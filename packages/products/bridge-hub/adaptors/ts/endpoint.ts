// 手写 TS/JS 端点 adaptor（Node / Bun / Deno 共用）。
// 只允许出现 manifest + 两个业务回调；帧编解码、stdio 循环、id、心跳、错误包装
// 全部来自 protocol/gen/ts/runtime.ts（由 gen.mjs 生成）。
import type { EndpointHooks, EndpointManifest } from '../../protocol/gen/ts/runtime.ts';
import { boot, currentRuntime } from '../../protocol/gen/ts/runtime.ts';
import { busError, E_INVALID_PARAMS, E_METHOD_NOT_FOUND } from '../../protocol/gen/ts/protocol.ts';

export const manifest: EndpointManifest = {
  provides: ['echo.ping', 'echo.upper'],
  subscribes: ['event.diag', 'event.agent_*'],
};

export const hooks: EndpointHooks = {
  onRequest(capability, params, correlationId) {
    if (capability === 'echo.ping') {
      return { result: { pong: true, params, correlationId, endpointId: 'ts-endpoint' } };
    }
    if (capability === 'echo.upper') {
      const text = (params as Record<string, unknown> | null)?.text;
      if (typeof text !== 'string') {
        return { error: busError(E_INVALID_PARAMS, 'text must be a string') };
      }
      return { result: { text: text.toUpperCase(), endpointId: 'ts-endpoint' } };
    }
    return { error: busError(E_METHOD_NOT_FOUND, 'unsupported capability') };
  },
  onEvent(topic, payload, from) {
    currentRuntime().publish('event.echo_ack', {
      endpointId: 'ts-endpoint',
      receivedTopic: topic,
      from,
      payload,
    });
  },
};

await boot(manifest, hooks);
