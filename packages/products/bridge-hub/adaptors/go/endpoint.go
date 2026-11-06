package main

// 手写 Go 端点 adaptor：只有 manifest + 两个业务回调。
// 帧编解码、stdio 循环、id、心跳、错误包装来自 protocol/gen/go/runtime.go。

import (
	"encoding/json"
	"strings"
)

func main() {
	Boot(
		EndpointManifest{
			Provides:   []string{"echo.ping", "echo.upper"},
			Subscribes: []string{"event.diag", "event.agent_*"},
		},
		EndpointHooks{
			OnRequest: func(capability string, params json.RawMessage, correlationID string) RequestOutcome {
				switch capability {
				case "echo.ping":
					return RequestOutcome{Result: map[string]any{
						"pong":          true,
						"params":        json.RawMessage(params),
						"correlationId": correlationID,
						"endpointId":    "go-endpoint",
					}}
				case "echo.upper":
					var body struct {
						Text string `json:"text"`
					}
					if err := json.Unmarshal(params, &body); err != nil || body.Text == "" {
						return RequestOutcome{Error: BusError(E_INVALID_PARAMS, "text must be a string")}
					}
					return RequestOutcome{Result: map[string]any{
						"text":       strings.ToUpper(body.Text),
						"endpointId": "go-endpoint",
					}}
				default:
					return RequestOutcome{Error: BusError(E_METHOD_NOT_FOUND, "unsupported capability")}
				}
			},
			OnEvent: func(topic string, payload json.RawMessage, from string) {
				ack, err := json.Marshal(map[string]any{
					"endpointId":    "go-endpoint",
					"receivedTopic": topic,
					"from":          from,
				})
				if err == nil {
					CurrentRuntime().Publish("event.echo_ack", ack)
				}
			},
		},
	)
}
