/**
 * Node bridge 请求分发入口，处理握手、内建方法与插件调用。
 *
 * 主要导出：createHandshakeResponse、handleRequest、handleExchange、readyEvent。
 */
import type { BridgeEvent } from '../protocol/events.js';
import { createPluginRegistry } from '../core/plugin-registry.js';
import type { BridgeRequest, HandshakeRequest } from '../protocol/request.js';
import type { BridgeExchange, BridgeResponse, HandshakeResponse } from '../protocol/response.js';
import { createHandshakeResponse as createBuiltinHandshakeResponse, handleBuiltinRequest } from './builtin-handlers.js';
import { handlePluginRequest } from './plugin-dispatch.js';

const registry = createPluginRegistry();

export function createHandshakeResponse(request: HandshakeRequest): HandshakeResponse {
  return createBuiltinHandshakeResponse(registry, request);
}

export async function handleRequest(request: BridgeRequest): Promise<BridgeResponse> {
  return (await handleExchange(request)).response;
}

export async function handleExchange(request: BridgeRequest): Promise<BridgeExchange> {
  registry.metrics.recordRequest();
  const builtin = await handleBuiltinRequest(registry, request);
  return builtin ?? handlePluginRequest(registry, request);
}

export function readyEvent(): BridgeEvent<{ plugins: string[] }> {
  const snapshot = registry.snapshot();
  return {
    method: 'event.ready',
    params: {
      plugins: snapshot.pluginNames,
    },
  };
}
