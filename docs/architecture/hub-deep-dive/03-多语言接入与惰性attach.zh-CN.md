# 03 多语言接入与惰性 attach：五种语言，五种接法

同样是"接进 Hub"，Node、Python、Go、Dart、Rust 用了五种不同的办法。差别集中在两个问题上：

1. **怎么把业务脚本的 `emit`（输出 1:1 envelope）改成 Hub 的 `publish`？**
2. **什么时候付出启动/编译成本？**

这篇逐一精讲，最后给对比表。

## 核心矛盾

业务脚本（如 `lan_py_bridge.py`、`lan_go_bridge.go`、`lan_dart_bridge.dart`、`lania-rs-bridge`）原本是为 1:1 协议写的：业务逻辑产出事件，经过 `emit → emit_event → emit_diag → emit_response` 链写到 stdout。

Hub 要复用这些业务（**不重写工具逻辑**），就得让同样的业务代码把事件写 BusFrame publish、把结果写 BusFrame response。语言特性不同，改法就不同：

- **动态语言（Python）**：运行时替换模块里的 `emit` 函数引用——monkey-patch
- **静态语言（Go/Dart/Rust）**：在 `emit` 内部预置一个环境变量分支——hubMode

---

## §1 Python：monkey-patch（动态语言红利）

**`adaptors/py/hub_endpoint.py:15-20`** 把业务脚本目录塞进 `sys.path`，导入真实业务模块：

```python
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lan_py_bridge as biz  # noqa: E402
from lania_bridge_runtime import boot, current_runtime  # noqa: E402
```

**`:22-25`** 能力清单直接取自业务脚本，不写两遍：

```python
MANIFEST = {"provides": list(biz.METHODS), "subscribes": ["event.*"]}
```

**`:28-47` `_hub_emit(envelope)`** 把 1:1 event envelope 转成 BusFrame publish（response envelope 忽略，因为 Hub 响应由 hooks 返回值承载）：

```python
def _hub_emit(envelope: dict) -> None:
    if envelope.get("type") != "event":
        return
    payload = envelope.get("payload") or {}
    method = payload.get("method")
    if method:
        current_runtime().publish(method, payload.get("params"))
```

**`:50-51` 关键一行——monkey-patch**：

```python
# 关键：重定向业务脚本的输出函数，让它产生 Hub 事件而非 1:1 envelope。
biz.emit = _hub_emit
```

**为什么能这样做？** 看业务脚本 `lan_py_bridge.py:92-126`：

```python
def emit(envelope):                      # :92 写 stdout（1:1 envelope）
    sys.stdout.write(json.dumps(envelope, ensure_ascii=False) + "\n")
def emit_event(method, params, request_id="bridge"):   # :97
    emit({"type": "event", ...})
def emit_diag(...):                      # :101
    emit_event("event.diag", payload, request_id)
```

业务函数（如 `handle_lint` 在 `:508`）调 `emit_diag`，内部通过**全局名字查找** `emit`。把 `biz.emit` 换掉后，整条输出链即时改道，业务源码一个字都不用改。这是 Python 的 module 可变、函数一等公民带来的独特红利。

**`:54-80` `Hooks.on_request`** 直接分发到业务函数：

```python
if capability.startswith("lint."):
    adaptor = capability.split(".", 1)[1]
    result = biz.handle_lint(adaptor, params, correlation_id)
    return {"result": result}
if capability == "build.python":
    result = biz.handle_build(params, correlation_id)
```

---

## §2 Go / Dart：hubMode 分支（静态语言）

静态语言的函数绑定在编译期确定，没法像 Python 那样运行时替换模块成员。做法是在 `emit` 里预置分支。

### Go

**`lan_go_bridge.go:240-242`** 包级变量，启动时读一次环境变量：

```go
// hubMode=true 时以 Hub 端点身份运行（BusFrame），否则保持 1:1 envelope。
// 业务函数（handleLint/handleBuild）两者共用，零改动。
var hubMode = os.Getenv("LANIA_BRIDGE_MODE") == "hub"
```

**`:257-275` `emit()`**：hub 模式下只把 event envelope 转 publish，response / handshake 被吞掉；非 hub 走原 1:1：

```go
func emit(envelope map[string]any) {
    if hubMode {
        if envelope["type"] != "event" { return }
        payload, _ := envelope["payload"].(map[string]any)
        method, _ := payload["method"].(string)
        writeFrame(map[string]any{"kind": "publish", "topic": method, "payload": payload["params"]})
        return
    }
    writeFrame(envelope)
}
```

`emitEvent/emitDiag/emitResponse`（`:277/:288/:311`）都经 `emit`，业务侧无需改动。

**`:818-821` `serve()`** 分流：`if hubMode { return serveHub() }`。

**`:872-945` `serveHub()`** 新增的 Hub 循环：announce 真实能力 → 逐行读 BusFrame request → 复用 `handleLint/handleBuild` → 组装 BusFrame response：

```go
writeFrame(map[string]any{
    "kind": "announce", "endpointId": endpointID,
    "provides": bridgeMethods, "subscribes": []string{"event.*"},
})
...
case strings.HasPrefix(method, "lint."):
    adaptor := strings.TrimPrefix(method, "lint.")
    emitEvent("event.lint_start", ..., id)   // hubMode 下自动变 publish
    res := handleLint(adaptor, pm, id)
...
resp := map[string]any{"kind": "response", "id": id, "result": result}
writeFrame(resp)
```

`bridgeMethods`（`:49-56`）= `bridge.ping/heartbeat/shutdown`、`lint.go-vet`、`lint.golangci-lint`、`build.go`。

### Dart

**`lan_dart_bridge.dart:72-74`** 顶层变量，与 Go 同构：

```dart
final bool _hubMode = Platform.environment['LANIA_BRIDGE_MODE'] == 'hub';
```

**`:76-105` `_emit()`** hub 模式下把 envelope 原地改写成 publish 帧。注意它用 `_outbox` 串行 FIFO 队列——保证 event 先于 response 且每次 flush：

```dart
void _emit(Map<String, Object?> envelope) {
  if (_hubMode) {
    if (envelope['type'] != 'event') return;
    final Object? payload = envelope['payload'];
    ...
    envelope = <String, Object?>{'kind': 'publish', 'topic': method, 'payload': payload['params']};
  }
  final String line = jsonEncode(envelope);
  _outbox = _outbox.then((_) async { stdout.writeln(line); await stdout.flush(); });
}
```

**`:794-878` `_serveHub()`**：announce `_methods` → `await for` 逐行读 BusFrame → 复用 `_handleLint/_handleBuild` → `_writeFrame` 回 response。这里有一处可学习的健壮性细节——lint / build 分支都包了 try/catch，把异常转成 BusFrame response error，避免单个工具崩溃杀掉整个进程（`:838-856`）。

`_writeFrame`（`:108-118`）专供 announce/response；`main()`（`:880-889`）退出前 `await _outbox` 排空队列。

---

## §3 Rust：自启动（current_exe + 隐藏参数）

Rust 端点不找解释器、也不编译源码——**它就是当前这个 CLI 二进制自己**，通过一个隐藏子命令进入 bridge 模式。

**`lania-bridge-host/src/lib.rs:222` `attach_rust_hub()`**：

```rust
pub fn attach_rust_hub(&mut self, id: &str) -> io::Result<()> {
    let cwd = std::env::current_dir()?;
    let spec = lania_rs_bridge::launch_spec(&cwd)...;
    let mut command = Command::new(spec.program);
    command.args(spec.args).current_dir(spec.dir)
        .env("LANIA_BRIDGE_MODE", "hub")
        .env("LANIA_ENDPOINT_ID", id);
    self.attach_command(id, command)
}
```

**`lania-rs-bridge/src/lib.rs:86-105` `launch_spec()`** 是"自己再起一个自己"的核心：

```rust
// 默认就是「自己再起一个自己」：`current_exe + [BRIDGE_ARG]`。
match std::env::var_os(BRIDGE_BINARY_ENV).filter(|v| !v.is_empty()) {
    Some(explicit) => (PathBuf::from(explicit), Vec::new()),
    None => (std::env::current_exe()?, vec![BRIDGE_ARG.to_string()]),
}
```

常量在 `lib.rs:35-36`：`BRIDGE_ARG = "__lania_rs_bridge"`，`BRIDGE_BINARY_ENV = "LANIA_RS_BRIDGE"`（可覆盖）。

**闭环入口** — `lania-cli/src/main.rs:30-42`：在任何宿主装配之前短路：

```rust
// B11：同一个二进制兼作 Rust 运行时 bridge（`lan __lania_rs_bridge`）。
if maybe_run_rust_bridge()? { return Ok(()); }
```

`bridge_setup.rs:25` `maybe_run_rust_bridge()` 判定 → `run_stdio_bridge()` → 按协议 exit code `process::exit`。

**emit 改道** — `lania-rs-bridge/src/lib.rs:388-413`：与 Go/Dart 同构，区别是 `hub_mode()` 是**函数、每次调用实时读 env**：

```rust
fn hub_mode() -> bool {
    std::env::var("LANIA_BRIDGE_MODE").as_deref() == Ok("hub")
}
fn emit(value: &Value) {
    if hub_mode() {
        if value["type"].as_str() != Some("event") { return; }
        let method = value["payload"]["method"].as_str().unwrap_or("");
        if method.is_empty() { return; }
        write_frame(&json!({"kind":"publish","topic":method,"payload":value["payload"]["params"]}));
        return;
    }
    write_frame(value);
}
```

**`run_hub_bridge()`**（`lib.rs:820-902`）：announce `METHODS`（`lint.clippy`/`build.rust`）→ 读 BusFrame request → 复用 `handle_lint/handle_build` → 写 BusFrame response。

---

## §4 解释器/程序定位的健壮性

接进 Hub 前要先找到"怎么启动这个运行时"。三处代码体现了同一思路：**显式环境变量覆盖优先 → 项目本地约定 → PATH 兜底**，并针对各生态的坑做专门处理。

### Python：`resolve_python`（venv / PATH）

`lania-py-bridge/src/lib.rs:25-44` `python_candidates()`：Windows 候选 `python.exe/python3.exe/py.exe`，Unix `python3/python`；先遍历 `.venv`/`venv` × `Scripts`/`bin`：

```rust
for venv in [".venv", "venv"] {
    for sub in ["Scripts", "bin"] {
        for executable in executables {
            candidates.push(cwd.join(venv).join(sub).join(executable));
        }
    }
}
```

`lib.rs:46-65` `resolve_python_unlocked`：`LANIA_PYTHON` 显式覆盖优先，只做存在性判断不执行 `--version`。Hub 侧 `attach_python_script`（`lania-bridge-host/src/lib.rs:152`）另认 `LANIA_BRIDGE_PYTHON` 覆盖。

> 这处是本次改造修过的真实 bug：原来硬编码 `"python"` 的机器只有 `python3`，导致 Python 端点根本没启动。

### Dart：`resolve_dart`（FVM / Flutter / .bat）

`lania-dart-bridge/src/lib.rs:45-59` `project_candidates()`：找 `.fvm/flutter_sdk`、`.fvm/dart_sdk`、`.flutter`，每处探 `bin/<exe>` 和 `bin/cache/dart-sdk/bin/<exe>`。

`lib.rs:61-70` `sibling_real_dart(path)` 处理 Windows 批处理坑：命中 `dart.bat/.cmd` 时优先取旁边的真实 `dart.exe`——避免 `CreateProcess` 直接拉批处理报错、少起一层 cmd。

`lib.rs:72-92` `resolve_dart`：优先级 `LANIA_DART` > 项目 FVM 候选 > PATH（再经 sibling 修正）。

### Go：`ensure_binary`（缓存二进制）

`lania-go-bridge/src/lib.rs:98-160` `ensure_binary()`：三级查找——`LANIA_GO_BRIDGE` 显式预编译 → 与 `current_exe` 同目录的平台包 → 缓存产物（校验源码 hash 与当前一致）。未命中才 `go build`，且并发安全：先编译到带 pid 的 staging 文件，再 `rename` 到目标，`CGO_ENABLED=0`。

```rust
fn cache_key() -> String {
    content_hash(&format!("{}\n{}", env!("CARGO_PKG_VERSION"), BRIDGE_SOURCE))
}
...
std::fs::rename(&staging, &target)...;
std::fs::write(hash_path(), cache_key())...;
```

`attach_go`（`lania-bridge-host/src/lib.rs:188`）复用 `ensure_binary()` 而非 `go run`，避免每次重编；同一二进制用 `LANIA_BRIDGE_MODE=hub` 切模式。

---

## §5 惰性 attach：用到才启动

**状态字段** — `provider.rs:86-93`：

```rust
struct ProviderState {
    host: BridgeHost,
    client: Endpoint,
    languages: BTreeMap<String, BridgeLaunchSpec>,
    /// 已尝试惰性 attach 的语言端点 id（避免每个请求重复 spawn/编译）。
    attached_languages: BTreeSet<String>,
    providers: Vec<ProviderSlot>,
}
```

**触发点** — `call_sync_stream`（`provider.rs:201`）在每次发 request 之前：

```rust
211→        ensure_language_endpoint(state, method);
212→        let id = state.client.request(method, params);
```

**决策函数** — `provider.rs:263-290`：

```rust
264→    let (id, attach) = if method.starts_with("lint.go-")
265→        || method.starts_with("lint.golangci") || method == "build.go" {
268→        (GO_ENDPOINT_ID, BridgeHost::attach_go as ...)
269→    } else if method.starts_with("lint.dart-") || method == "build.dart" {
270→        (DART_ENDPOINT_ID, BridgeHost::attach_dart as ...)
271→    } else if method == "lint.clippy" || "lint.rust" || "build.rust" {
272→        (RS_ENDPOINT_ID, BridgeHost::attach_rust_hub as ...)
273→    } else { return; }                 // Python/Node 构造时已 attach
276→    if state.attached_languages.contains(id) { return; }
280→    state.attached_languages.insert(id.to_string());   // 先标记，失败也不重试
281→    if attach(&mut state.host, id).is_err() { return; }
286→    while !state.host.hub().is_announced(id) && ... { state.host.pump_until_idle(50); }
```

设计动机（注释在 `provider.rs:166`）：`ensure_binary()` 首次可能要 `go build`（数十秒），若构造 Hub 时无条件 attach，纯 Node 命令也要付这个成本。改为按 capability 前缀触发 + `BTreeSet` 去重后，成本只在该语言能力**首次被真实调用**时付一次，失败也只尝试一次。

**对照**：Node（`:137-153`）和 Python（`:158`）在构造时就 attach 并等 announce，因为它们启动快、是高频路径。

---

## §6 echo 骨架 vs 真实业务：一次重要的分离

`endpoint.*`（echo 骨架）和 `hub_endpoint.*`（真实业务）是分开的——这是一个从测试中得到的教训。

- echo 骨架：`adaptors/{ts,py,go,dart}/endpoint.*` + Rust 进程内 `EchoHandler`，只提供 `echo.ping/echo.upper/event.echo_ack`，给协议契约测试用（如 `tests/stdio_endpoints.rs:69`）。
- 真实业务：`hub_endpoint.py`、`lan_go_bridge.go`、`lan_dart_bridge.dart`、`lania-rs-bridge`，提供 `lint.*`/`build.*`。

**常量层分离** — `lania-bridge-host/src/lib.rs:31-36`：

```rust
const PY_ENDPOINT: &str = embedded!("adaptors/py/endpoint.py");
/// 真实业务端点（echo 骨架 endpoint.py 保留给协议契约测试）。
const PY_HUB_ENDPOINT: &str = embedded!("adaptors/py/hub_endpoint.py");
```

**同名不同脚本** — `lib.rs:137-172`：

```rust
/// echo 骨架端点（协议契约测试用）。
pub fn attach_python(&mut self, id: &str) -> io::Result<()> {
    self.attach_python_script(id, "endpoint.py")
}
/// 真实业务端点：lint.mypy/ruff/black/flake8、build.python（Hub 单轨）。
pub fn attach_python_hub(&mut self, id: &str) -> io::Result<()> {
    self.attach_python_script(id, "hub_endpoint.py")
}
```

生产路径 `provider.rs:158` 用 `attach_python_hub`，测试路径用 `attach_python`。两者共用同一个解释器解析与 `attach_command`。

Go/Dart/Rust 没有独立的 `hub_endpoint.*`——它们把 1:1 与 Hub 双模做进**同一个业务脚本**，靠环境变量切；`endpoint.go/dart/rs` 只作为 echo 骨架被 stage。

> 教训：如果把真实业务直接塞进 `endpoint.py`，所有基于 echo 契约的协议测试都会失效。分离之后，协议层测试和业务层测试各测各的，互不干扰。

---

## 五种语言接入对比表

| 语言       | Hub 接入模式                               | emit → publish 的机制                                          | 程序定位                                                       | attach 时机                                               | 业务脚本                               |
| ---------- | ------------------------------------------ | -------------------------------------------------------------- | -------------------------------------------------------------- | --------------------------------------------------------- | -------------------------------------- |
| **Node**   | 环境变量 hub 模式，`runHubEndpoint()` 接管 | `LANIA_BRIDGE_MODE=hub` → `runHubEndpoint()`（`client.rs:37`） | `bridge_launch_spec()`（`client.rs:44`）                       | **构造时** spawn 等 announce（`provider.rs:137`）         | 真实 node bridge                       |
| **Python** | **monkey-patch**（动态语言特有）           | `biz.emit = _hub_emit`（`hub_endpoint.py:51`）                 | `resolve_python`：venv/PATH + `LANIA_PYTHON`（`py lib.rs:26`） | **构造时** `attach_python_hub`（`provider.rs:158`）       | `hub_endpoint.py` + `lan_py_bridge.py` |
| **Go**     | **hubMode 分支**（静态）                   | 包级 `var hubMode`（`:242`）+ `emit` 内分支（`:257`）          | `ensure_binary()`：预编译/缓存 `go build`（`go lib.rs:98`）    | **惰性**（`lint.go-*`/`build.go`，`provider.rs:264`）     | `lan_go_bridge.go` 单脚本双模          |
| **Dart**   | **\_hubMode 分支**（静态）                 | 顶层 `final _hubMode`（`:74`）+ `_emit`（`:76`）               | `resolve_dart`：FVM/Flutter/.bat→exe（`dart lib.rs:45`）       | **惰性**（`lint.dart-*`/`build.dart`，`provider.rs:269`） | `lan_dart_bridge.dart` 单脚本双模      |
| **Rust**   | **自启动**（current_exe + 隐藏参数）       | 函数 `hub_mode()` 实时读 env（`:390`）+ `emit`（`:395`）       | `launch_spec()`：`current_exe()`+`__lania_rs_bridge`（`:90`）  | **惰性**（`lint.clippy`/`build.rust`，`provider.rs:271`） | 同一 `lan` 二进制双模                  |

**共同点**：所有端点最终都以 BusFrame 经 `BridgeHost::attach_command` → StdioChannel 接入 Hub（Rust echo 例外走进程内 channel）；Hub 只按 capability 路由，帧里不含任何语言字段。

## 一句话总结

五种语言的差异集中在两点：**把业务 `emit` 改造成 Hub `publish` 的手法**（monkey-patch vs 环境变量分支 vs 自启动），以及**何时付出启动成本**（Node/Python 构造时、Go/Dart/Rust 惰性）。理解了这个二维对比，就理解了整套多语言接入设计。
