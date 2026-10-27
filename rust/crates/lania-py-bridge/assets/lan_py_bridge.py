#!/usr/bin/env python3
"""lan Python bridge (jsonl stdio protocol).

设计约束（docs/design/multi-runtime-bridge.zh-CN.md §3.1）：

1. **stdout 只输出协议 envelope**，所有日志走 stderr —— 否则上游 jsonl 解析失败。
2. 事件必须先于对应 request 的 response 发出。
3. 逐行 JSON，禁止跨行、禁止 BOM。
4. 收到 stdin EOF 必须退出（父进程消失时自杀，避免孤儿进程）。
5. 长任务必须保持 stdout 打开并持续 emit。
6. 被劫持的工具输出走 `event.diag`。
7. 产出统一 `Diagnostic`（字段与 Rust `Diagnostic` / TS `diagnostics.ts` 完全一致）。

单文件、零第三方依赖（除被调用的工具本身）：随 `lan-py-bridge` 二进制分发，
由 Rust 侧用 `python -u script` 拉起，见 `rust/crates/lania-py-bridge/src/lib.rs`。
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from typing import Any, Dict, Iterable, List, Optional, Tuple

BRIDGE_NAME = "lan-py-bridge"
PROTOCOL_VERSION = "0.1.0"
HEARTBEAT_INTERVAL_MS = 10_000
MAX_PENDING_EVENTS = 32

# 与 node-bridge 的 `BridgeFailureStrategy` 对齐。
FAILURE_STRATEGY = "reconnect"

# 与 node-bridge 的 `BRIDGE_EVENTS` 保持交集：本 bridge 只声明自己会发出的事件。
EVENTS = [
    "event.ready",
    "event.log",
    "event.diag",
    "event.lint_start",
    "event.lint_result",
    "event.lint_summary",
    "event.shutdown",
    "event.heartbeat",
]

METHODS = [
    "bridge.ping",
    "bridge.heartbeat",
    "bridge.shutdown",
    "lint.mypy",
    "lint.ruff",
    "lint.black",
    "lint.flake8",
    "build.python",
]

# adaptor id → 进程内可用的 API 名。没有 API 的工具退化为 spawn（§3.3）。
API_TOOLS = {"mypy", "black", "flake8"}
# 工具名 → 默认命令行参数（与 `toolchain.ts` 的 lintTools[].args 同值）。
DEFAULT_ARGS: Dict[str, List[str]] = {
    "mypy": ["."],
    "ruff": ["check", "--output-format", "json", "."],
    "black": ["--check", "."],
    "flake8": ["."],
}

# adaptor → 缺少工具时的提示级别；只有「工具本身不存在」才算 warning。
MISSING_TOOL_MESSAGE = "{tool} is not installed in this Python environment"

_seq = 0


# ---------------------------------------------------------------------------
# 协议输出
# ---------------------------------------------------------------------------


def log(message: str) -> None:
    """所有非协议输出必须走 stderr（§3.1 约定 1）。"""
    sys.stderr.write(message + "\n")
    sys.stderr.flush()


def next_seq() -> int:
    global _seq
    _seq += 1
    return _seq


def emit(envelope: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(envelope, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def emit_event(method: str, params: Dict[str, Any], request_id: str = "bridge") -> None:
    emit({"type": "event", "requestId": request_id, "payload": {"method": method, "params": params}})


def emit_diag(
    request_id: str,
    text: str,
    *,
    stream: str = "stderr",
    channel: str = "lint",
    level: str = "info",
    tool: Optional[str] = None,
) -> None:
    """被劫持的工具输出 → `event.diag`（§7.2 R1）。"""
    payload: Dict[str, Any] = {
        "source": "tool" if tool else "runtime",
        "stream": stream,
        "channel": channel,
        "level": level,
        "text": text,
        "seq": next_seq(),
        "ts": int(time.time() * 1000),
    }
    if tool:
        payload["tool"] = tool
    emit_event("event.diag", payload, request_id)


def emit_response(request_id: str, result: Any = None, error: Optional[Dict[str, Any]] = None) -> None:
    emit({"type": "response", "payload": {"id": request_id, "result": result, "error": error}})


def error_payload(code: str, message: str) -> Dict[str, Any]:
    return {"code": code, "message": message, "data": None}


# ---------------------------------------------------------------------------
# 工具定位：项目虚拟环境优先，其次 PATH（§6.4 的探测顺序必须保留）
# ---------------------------------------------------------------------------


def _venv_candidates(cwd: str, tool: str) -> List[str]:
    executable = tool + ".exe" if os.name == "nt" else tool
    return [
        os.path.join(cwd, ".venv", "Scripts", executable),
        os.path.join(cwd, ".venv", "bin", tool),
        os.path.join(cwd, "venv", "Scripts", executable),
        os.path.join(cwd, "venv", "bin", tool),
    ]


def resolve_tool(cwd: str, tool: str) -> Optional[str]:
    for candidate in _venv_candidates(cwd, tool):
        if os.path.isfile(candidate):
            return candidate
    return shutil.which(tool)


def python_executable(cwd: str) -> str:
    """项目虚拟环境里的解释器优先：`mypy.api` 必须与被检查项目同一个解释器。"""
    for candidate in _venv_candidates(cwd, "python"):
        if os.path.isfile(candidate):
            return candidate
    return sys.executable


# ---------------------------------------------------------------------------
# 诊断归一化（与 Rust `normalize_diagnostics` / TS `normalizeDiagnostics` 一致）
# ---------------------------------------------------------------------------

_SEVERITY_ORDER = {"error": 0, "warning": 1, "info": 2, "hint": 3}


def make_diagnostic(
    tool: str,
    *,
    severity: str,
    message: str,
    file: Optional[str] = None,
    line: Optional[int] = None,
    col: Optional[int] = None,
    end_line: Optional[int] = None,
    end_col: Optional[int] = None,
    rule_id: Optional[str] = None,
    url: Optional[str] = None,
    fix_kind: Optional[str] = None,
) -> Dict[str, Any]:
    """字段名与 Rust `Diagnostic`（serde camelCase 缺省）逐一对齐。"""
    diagnostic: Dict[str, Any] = {"tool": tool, "severity": severity, "message": message}
    if rule_id:
        diagnostic["id"] = rule_id
    if file:
        diagnostic["file"] = file
    if line is not None:
        start: Dict[str, Any] = {"line": line, "col": col if col is not None else 1}
        # `end` 只在两端都拿到时才写：字段可选，缺省表示行级定位。
        rng: Dict[str, Any] = {"start": start}
        if end_line is not None:
            end: Dict[str, Any] = {"line": end_line, "col": end_col if end_col is not None else start["col"]}
            rng["end"] = end
        diagnostic["range"] = rng
    if url:
        diagnostic["url"] = url
    if fix_kind:
        diagnostic["fix"] = {"available": fix_kind != "none", "kind": fix_kind}
    return diagnostic


def _dedup_key(diagnostic: Dict[str, Any]) -> Tuple[str, str, int, int, str]:
    rng = diagnostic.get("range") or {}
    start = rng.get("start") or {}
    return (
        diagnostic.get("tool", ""),
        diagnostic.get("file", ""),
        int(start.get("line", 0) or 0),
        int(start.get("col", 0) or 0),
        diagnostic.get("id", ""),
    )


def normalize_diagnostics(diagnostics: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    seen = set()
    deduped: List[Dict[str, Any]] = []
    for diagnostic in diagnostics:
        key = _dedup_key(diagnostic)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(diagnostic)

    def sort_key(diagnostic: Dict[str, Any]) -> Tuple[Any, ...]:
        rng = diagnostic.get("range") or {}
        start = rng.get("start") or {}
        return (
            diagnostic.get("file", ""),
            int(start.get("line", 0) or 0),
            int(start.get("col", 0) or 0),
            _SEVERITY_ORDER.get(diagnostic.get("severity", "info"), 3),
            diagnostic.get("id", ""),
        )

    return sorted(deduped, key=sort_key)


def files_from_diagnostics(diagnostics: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """`files` 由 `diagnostics` 派生，保证两个字段永不失配。"""
    by_file: Dict[str, Dict[str, Any]] = {}
    for diagnostic in diagnostics:
        file_path = diagnostic.get("file") or "."
        entry = by_file.setdefault(file_path, {"filePath": file_path, "errors": 0, "warnings": 0})
        if diagnostic.get("severity") == "error":
            entry["errors"] += 1
        else:
            entry["warnings"] += 1
    return list(by_file.values())


def lint_result(
    adaptor: str,
    implementation: str,
    diagnostics: List[Dict[str, Any]],
    *,
    message: Optional[str] = None,
) -> Dict[str, Any]:
    normalized = normalize_diagnostics(diagnostics)
    files = files_from_diagnostics(normalized)
    result: Dict[str, Any] = {
        "adaptor": adaptor,
        "implementation": implementation,
        "errors": sum(item["errors"] for item in files),
        "warnings": sum(item["warnings"] for item in files),
        "files": files,
        "diagnostics": normalized,
    }
    if message:
        result["message"] = message
    return result


# ---------------------------------------------------------------------------
# 各工具的 API 实现
# ---------------------------------------------------------------------------


def _mypy_pythonpath(cwd: str) -> str:
    """让子进程能 `import mypy`：项目里的 venv 与宿主解释器都要能找到。"""
    parts = [cwd]
    current = os.environ.get("PYTHONPATH")
    if current:
        parts.append(current)
    return os.pathsep.join(parts)


def _run_python_api(cwd: str, adaptor: str, args: List[str]) -> Tuple[Optional[str], str]:
    """在项目解释器里调用工具 API，返回 `(stdout, stderr)`。

    为什么要起子进程：`mypy` / `black` / `flake8` 都持有模块级状态（mypy 的类型缓存、
    black 的 FileMode），同一解释器里连续调用会互相污染。一次 lint = 一个干净解释器，
    但**解析仍留在主进程**，保证 API 路径与 spawn 路径产出同一份 `Diagnostic`（§5.3）。
    """
    command = [
        python_executable(cwd),
        os.path.abspath(__file__),
        "--worker",
        adaptor,
        cwd,
    ] + args
    env = dict(os.environ)
    env["PYTHONPATH"] = _mypy_pythonpath(cwd)
    env["PYTHONUNBUFFERED"] = "1"
    env["FORCE_COLOR"] = "0"
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except OSError as error:
        return None, f"failed to start python helper: {error}"
    stderr = completed.stderr.decode("utf-8", "replace")
    # 退出码 2 表示「工具 API 层不可用」（未安装 / 版本不匹配），调用方据此回落 spawn。
    if completed.returncode >= 2:
        return None, stderr
    return completed.stdout.decode("utf-8", "replace"), stderr


def run_in_process_api(cwd: str, adaptor: str, args: List[str]) -> Optional[List[Dict[str, Any]]]:
    """返回 `None` 表示「这个环境里没有可用 API」，调用方据此回落 spawn。"""
    if adaptor not in API_TOOLS:
        return None
    stdout, stderr = _run_python_api(cwd, adaptor, args)
    if stdout is None:
        # API 不可用（未安装 / 版本不匹配 / 参数错误）：交给 spawn 回落，而不是报错。
        if stderr:
            log(f"{adaptor}: python api unavailable, falling back to spawn")
        return None
    if adaptor == "mypy":
        return parse_mypy(stdout + "\n" + stderr)
    if adaptor == "black":
        return parse_black(stdout + "\n" + stderr)
    if adaptor == "flake8":
        return parse_flake8(stdout + "\n" + stderr)
    return None


def spawn_raw(cwd: str, tool: str, args: List[str]) -> Tuple[str, str, int]:
    executable = resolve_tool(cwd, tool)
    if not executable:
        raise FileNotFoundError(tool)
    env = dict(os.environ)
    env["FORCE_COLOR"] = "0"
    completed = subprocess.run(
        [executable] + args,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return (
        completed.stdout.decode("utf-8", "replace"),
        completed.stderr.decode("utf-8", "replace"),
        completed.returncode,
    )


# ---------------------------------------------------------------------------
# 解析器：与 `ts/packages/node-bridge/src/plugins/lint/parsers.ts` 逐字段对齐
# ---------------------------------------------------------------------------


def parse_mypy(output: str) -> List[Dict[str, Any]]:
    """`file:line: severity: message  [code]`；`note:` 行不算独立诊断。

    mypy 的消息体里可能含 `": "`（例如 `Incompatible types in assignment (...)`
    之后还有说明），因此用 `split(": ", 2)` 只切前两段，
    再从第三段里再拆出 `severity`。文件路径本身可能带盘符（`C:\\a\\b.py:5`），
    所以定位段固定按**最后一个** `:` 拆行号。
    """
    diagnostics: List[Dict[str, Any]] = []
    for line in output.splitlines():
        head, _, rest = line.partition(": ")
        severity, sep, body = rest.partition(": ")
        if not sep or severity not in ("error", "warning"):
            continue
        file_path, _, line_number = head.rpartition(":")
        if not line_number.isdigit():
            continue
        message = body.strip()
        rule_id: Optional[str] = None
        if message.endswith("]") and " [" in message:
            head_text, _, tail = message.rpartition(" [")
            rule_id = tail[:-1] or None
            message = head_text.strip()
        diagnostics.append(
            make_diagnostic(
                "mypy",
                severity="error" if severity == "error" else "warning",
                message=message,
                file=file_path,
                line=int(line_number),
                col=1,
                rule_id=rule_id,
            )
        )
    return diagnostics


def parse_black(output: str) -> List[Dict[str, Any]]:
    """`would reformat <file>` → 文件级 warning（black 不提供行列号）。"""
    diagnostics: List[Dict[str, Any]] = []
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.lower().startswith("would reformat "):
            diagnostics.append(
                make_diagnostic(
                    "black",
                    severity="warning",
                    message="file would be reformatted",
                    file=stripped[len("would reformat ") :].strip(),
                )
            )
    return diagnostics


def parse_flake8(output: str) -> List[Dict[str, Any]]:
    """`file:line:col: CODE message`"""
    diagnostics: List[Dict[str, Any]] = []
    for line in output.splitlines():
        stripped = line.strip()
        pieces = stripped.split(":", 4)
        if len(pieces) < 5:
            continue
        file_path, line_number, col, rest = pieces[0], pieces[1], pieces[2], pieces[4]
        if not (line_number.strip().isdigit() and col.strip().isdigit()):
            continue
        rule = rest.strip().split(" ", 1)[0] if rest.strip() else ""
        message = rest.strip()[len(rule) :].strip() if rule else rest.strip()
        diagnostics.append(
            make_diagnostic(
                "flake8",
                severity="warning" if rule.startswith("W") else "error",
                message=message,
                file=file_path,
                line=int(line_number),
                col=int(col),
                rule_id=rule or None,
            )
        )
    return diagnostics


def parse_ruff_json(output: str) -> List[Dict[str, Any]]:
    """`ruff check --output-format json`：数组，每项含 code / location / end_location / fix。

    对齐 TS 参考实现 `parseRuffDiagnostics`（§12 验收：同一份 ruff 输出经
    Python bridge 与 JS 簇解析必须**深度相等**）：

    - `severity` 恒为 `error`：ruff 发现问题时退出码非 0，`code` 缺失只代表
      「没有规则号」，不代表「只是 warning」；
    - `fix.kind` 取 `fix.applicability`（`unsafe` / `safe`），不是一律 `safe`。
    """
    diagnostics: List[Dict[str, Any]] = []
    try:
        items = json.loads(output)
    except json.JSONDecodeError:
        return diagnostics
    if not isinstance(items, list):
        return diagnostics
    for item in items:
        if not isinstance(item, dict):
            continue
        location = item.get("location") or {}
        end_location = item.get("end_location") or {}
        fix = item.get("fix")
        fix_kind: Optional[str] = None
        if isinstance(fix, dict):
            fix_kind = "unsafe" if fix.get("applicability") == "unsafe" else "safe"
        diagnostics.append(
            make_diagnostic(
                "ruff",
                severity="error",
                message=str(item.get("message", "")).strip(),
                file=item.get("filename"),
                line=location.get("row"),
                col=location.get("column"),
                end_line=end_location.get("row"),
                end_col=end_location.get("column"),
                rule_id=item.get("code"),
                url=item.get("url"),
                fix_kind=fix_kind,
            )
        )
    return diagnostics


# ---------------------------------------------------------------------------
# 请求处理
# ---------------------------------------------------------------------------


def handle_lint(adaptor: str, params: Dict[str, Any], request_id: str) -> Dict[str, Any]:
    cwd = str(params.get("cwd") or os.getcwd())
    args = params.get("args")
    if not isinstance(args, list) or not args:
        args = DEFAULT_ARGS.get(adaptor, ["."])
    args = [str(item) for item in args]

    emit_diag(request_id, f"running {adaptor}", level="info", tool=adaptor)

    diagnostics = run_in_process_api(cwd, adaptor, args)
    implementation = "runtime"
    if diagnostics is None:
        # API 不可用。先确认工具是否存在于 PATH/venv：
        # 完全没装 → `fallback`（只是跳过，不算失败）；装了但没 API → spawn 仍然 `runtime`。
        if resolve_tool(cwd, adaptor) is None:
            return lint_result(
                adaptor,
                "fallback",
                [],
                message=MISSING_TOOL_MESSAGE.format(tool=adaptor),
            )
        try:
            stdout, stderr, exit_code = spawn_raw(cwd, adaptor, args)
        except FileNotFoundError:
            return lint_result(
                adaptor,
                "fallback",
                [],
                message=MISSING_TOOL_MESSAGE.format(tool=adaptor),
            )
        if adaptor == "ruff":
            diagnostics = parse_ruff_json(stdout)
        elif adaptor == "mypy":
            diagnostics = parse_mypy(stdout + "\n" + stderr)
        elif adaptor == "black":
            diagnostics = parse_black(stdout + "\n" + stderr)
        elif adaptor == "flake8":
            diagnostics = parse_flake8(stdout + "\n" + stderr)
        else:
            diagnostics = []
        if exit_code > 1 and not diagnostics:
            return lint_result(
                adaptor,
                "runtime",
                [],
                message=(stderr or stdout).strip()[:400] or f"{adaptor} failed",
            )

    result = lint_result(adaptor, implementation, diagnostics)
    result["tool"] = adaptor
    return result


def handle_build(params: Dict[str, Any], request_id: str) -> Dict[str, Any]:
    """`build.python`：优先 `build.ProjectBuilder` API，缺包时回落到 `python -m build`。"""
    cwd = str(params.get("cwd") or os.getcwd())
    python = python_executable(cwd)
    stdout, stderr = _run_python_api(cwd, "build", [cwd])
    if stdout is None:
        try:
            output, error, exit_code = spawn_raw(cwd, "python", ["-m", "build"])
            stdout, stderr = output, error
        except FileNotFoundError:
            return {"success": False, "message": "python is not available"}
    else:
        exit_code = 0
    for line in (stdout or "").splitlines():
        if line.strip():
            emit_diag(request_id, line, stream="stdout", channel="build")
    for line in (stderr or "").splitlines():
        if line.strip():
            emit_diag(
                request_id,
                line,
                stream="stderr",
                channel="build",
                level="error" if exit_code else "info",
            )
    return {"success": exit_code == 0, "exitCode": exit_code}


def handle_request(envelope: Dict[str, Any]) -> None:
    request = envelope.get("payload") or {}
    request_id = str(request.get("id", ""))
    method = str(request.get("method", ""))
    params = request.get("params") or {}
    if not isinstance(params, dict):
        params = {}

    if method == "bridge.ping":
        emit_response(request_id, {"ok": True, "bridgeName": BRIDGE_NAME})
        return
    if method == "bridge.heartbeat":
        emit_response(request_id, {"ok": True, "ts": int(time.time() * 1000)})
        return
    if method == "bridge.shutdown":
        emit_event("event.shutdown", {"reason": "shutdown"}, request_id)
        emit_response(request_id, {"ok": True})
        raise SystemExit(0)

    if method.startswith("lint."):
        adaptor = method.split(".", 1)[1]
        try:
            result = handle_lint(adaptor, params, request_id)
        except Exception as error:  # noqa: BLE001 - bridge 不能因为单个工具崩掉
            emit_response(request_id, None, error_payload("E_LINT_FAILED", str(error)))
            return
        emit_response(request_id, result)
        return

    if method == "build.python":
        try:
            result = handle_build(params, request_id)
        except Exception as error:  # noqa: BLE001
            emit_response(request_id, None, error_payload("E_BUILD_FAILED", str(error)))
            return
        emit_response(request_id, result)
        return

    emit_response(
        request_id,
        None,
        error_payload("E_UNSUPPORTED", f"unsupported method: {method}"),
    )


# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------


def handshake_response(request: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "protocolVersion": request.get("protocolVersion") or PROTOCOL_VERSION,
        "bridgeName": BRIDGE_NAME,
        "methods": METHODS,
        "events": EVENTS,
        "heartbeatIntervalMs": HEARTBEAT_INTERVAL_MS,
        "maxPendingEvents": MAX_PENDING_EVENTS,
        "failureStrategy": FAILURE_STRATEGY,
    }


def serve() -> int:
    """主循环：逐行读 stdin。EOF（父进程消失）即退出，避免孤儿进程（§3.1 约定 4）。"""
    emit_event(
        "event.ready",
        {"bridgeName": BRIDGE_NAME, "methods": METHODS, "pid": os.getpid()},
    )
    while True:
        line = sys.stdin.readline()
        if not line:
            return 0
        line = line.strip()
        if not line:
            continue
        try:
            envelope = json.loads(line)
        except json.JSONDecodeError as error:
            log(f"failed to parse envelope: {error}")
            continue
        kind = envelope.get("type")
        if kind == "handshake":
            emit({"type": "handshake", "payload": handshake_response(envelope.get("payload") or {})})
            continue
        if kind == "request":
            handle_request(envelope)
            continue
        if kind == "host_response":
            # 本 bridge 不发起 host RPC；收到响应说明上游版本不匹配，忽略即可。
            continue
        log(f"unsupported envelope type: {kind}")


def worker(adaptor: str, cwd: str, args: List[str]) -> int:
    """子进程模式：在干净解释器里跑一次工具 API，把原始输出写回 stdout。

    为什么需要独立解释器：工具 API 都持有模块级状态（mypy 的类型缓存、
    black 的 FileMode），同一进程里连续调用会互相污染。
    这里只做「导入 + 调一次 API + 打印原始输出」，解析仍在主进程完成，
    保证 API 路径与 spawn 路径产出**同一份** `Diagnostic`。
    """
    os.chdir(cwd)
    paths = [item for item in args if not item.startswith("-")]
    if adaptor == "mypy":
        import mypy.api  # noqa: PLC0415 - 懒加载：未安装时由调用方回落 spawn

        stdout, stderr, _status = mypy.api.run(args)
        sys.stdout.write(stdout)
        sys.stderr.write(stderr)
        return 0
    if adaptor == "black":
        import black  # noqa: PLC0415

        check_only = "--check" in args
        status = 0
        for path in paths or ["."]:
            for filename in black.gen_python_files(path, root=cwd):
                source = open(filename, encoding="utf-8").read()
                if black.format_str(source, mode=black.FileMode()) == source:
                    continue
                sys.stdout.write(f"would reformat {filename}\n")
                status = 1
                if not check_only:
                    with open(filename, "w", encoding="utf-8") as handle:
                        handle.write(black.format_str(source, mode=black.FileMode()))
        return status
    if adaptor == "flake8":
        import flake8.api.legacy as legacy  # noqa: PLC0415

        style_guide = legacy.get_style_guide()
        report = style_guide.check_files(paths or ["."])
        for statistic in report.get_statistics("E") + report.get_statistics("W"):
            sys.stdout.write(statistic + "\n")
        return 0
    sys.stderr.write(f"no python api for {adaptor}\n")
    return 2


def main(argv: List[str]) -> int:
    if argv and argv[0] == "--worker" and len(argv) >= 3:
        return worker(argv[1], argv[2], argv[3:])
    return serve()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
