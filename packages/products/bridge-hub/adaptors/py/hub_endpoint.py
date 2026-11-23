"""Python 真实业务端点（Hub / BusFrame 协议）。

帧编解码、stdio 循环、id、心跳、错误包装来自 protocol/gen/py/lania_bridge_runtime.py。
业务逻辑（lint.mypy/ruff/black/flake8、build.python）复用随 bridge 分发的
lan_py_bridge.py —— 与 1:1 bridge 共用同一份实现，避免「Python 工具知识写两遍」。

适配点：业务函数通过 `emit_diag` -> `emit_event` -> `emit` 写 1:1 envelope。
这里把 `lan_py_bridge.emit` 重定向到 Hub runtime 的 publish：event envelope
转成 BusFrame publish，response envelope 忽略（响应由 hooks 返回值承载）。
"""
import os
import sys
import time

# 让 import lan_py_bridge 成立（两文件同在 staging 的 adaptors/py/ 目录）。
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lan_py_bridge as biz  # noqa: E402
from lania_bridge_protocol import E_INTERNAL, bus_error  # noqa: E402
from lania_bridge_runtime import boot, current_runtime  # noqa: E402

MANIFEST = {
    "provides": list(biz.METHODS),
    "subscribes": ["event.*"],
}


def _hub_emit(envelope: dict) -> None:
    """1:1 envelope -> BusFrame publish。

    - event envelope：转成 `current_runtime().publish(method, params)`，
      这样诊断事件经 Hub 广播到 host 订阅者（event.*）。
    - response envelope：忽略；Hub 的响应由 hooks.on_request 返回值承载。
    """
    if not isinstance(envelope, dict):
        return
    if envelope.get("type") != "event":
        return
    payload = envelope.get("payload") or {}
    method = payload.get("method")
    if not method:
        return
    try:
        current_runtime().publish(method, payload.get("params"))
    except RuntimeError:
        # runtime 尚未 boot（不应发生），静默丢弃避免污染业务返回。
        pass


# 关键：重定向业务脚本的输出函数，让它产生 Hub 事件而非 1:1 envelope。
biz.emit = _hub_emit


class Hooks:
    def on_request(self, capability, params, correlation_id):
        params = params if isinstance(params, dict) else {}
        # 内建桥接方法：Hub 侧由本端点就地处理。
        if capability == "bridge.ping":
            return {"result": {"ok": True, "bridgeName": biz.BRIDGE_NAME}}
        if capability == "bridge.heartbeat":
            return {"result": {"ok": True, "ts": int(time.time() * 1000)}}
        if capability == "bridge.shutdown":
            current_runtime().publish("event.shutdown", {"reason": "shutdown"})
            return {"result": {"ok": True}}

        # 真实业务：复用 1:1 bridge 的 handle_lint / handle_build。
        try:
            if capability.startswith("lint."):
                adaptor = capability.split(".", 1)[1]
                result = biz.handle_lint(adaptor, params, correlation_id)
                return {"result": result}
            if capability == "build.python":
                result = biz.handle_build(params, correlation_id)
                return {"result": result}
        except Exception as error:  # noqa: BLE001 - bridge 不能因单个工具崩掉
            code = "E_LINT_FAILED" if capability.startswith("lint.") else "E_BUILD_FAILED"
            detail = biz.error_payload(code, str(error))
            return {"error": bus_error(detail["code"], detail["message"])}

        return {"error": bus_error(E_INTERNAL, f"unsupported capability: {capability}")}

    def on_event(self, topic, payload, sender):
        # host -> 端点方向的事件目前无业务消费。
        pass


boot(MANIFEST, Hooks())
