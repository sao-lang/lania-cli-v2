"""手写 Python 端点 adaptor。

只允许出现 manifest + 两个业务回调；帧编解码、stdio 循环、id、心跳、错误包装
全部来自 protocol/gen/py/lania_bridge_runtime.py（由 gen.mjs 生成）。
"""
from lania_bridge_protocol import (
    E_INVALID_PARAMS,
    E_METHOD_NOT_FOUND,
    bus_error,
)
from lania_bridge_runtime import boot, current_runtime

MANIFEST = {
    "provides": ["echo.ping", "echo.upper"],
    "subscribes": ["event.diag", "event.agent_*"],
}


class Hooks:
    def on_request(self, capability, params, correlation_id):
        if capability == "echo.ping":
            return {"result": {"pong": True, "params": params, "correlationId": correlation_id, "endpointId": "py-endpoint"}}
        if capability == "echo.upper":
            text = params.get("text") if isinstance(params, dict) else None
            if not isinstance(text, str):
                return {"error": bus_error(E_INVALID_PARAMS, "text must be a string")}
            return {"result": {"text": text.upper(), "endpointId": "py-endpoint"}}
        return {"error": bus_error(E_METHOD_NOT_FOUND, "unsupported capability")}

    def on_event(self, topic, payload, sender):
        current_runtime().publish(
            "event.echo_ack",
            {
                "endpointId": "py-endpoint",
                "receivedTopic": topic,
                "from": sender,
                "payload": payload,
            },
        )


boot(MANIFEST, Hooks())
