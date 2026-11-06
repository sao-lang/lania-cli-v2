/**
 * Node bridge 请求分发入口，处理握手、内建方法与插件调用。
 *
 * 主要导出：createHandshakeResponse、handleRequest、handleExchange、readyEvent。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type { BridgePluginEvent } from '../core/bridge-plugin.js';
import { createPluginRegistry } from '../core/plugin-registry.js';
import type { BridgeRequest, HandshakeRequest } from '../protocol/request.js';
import type { BridgeExchange, BridgeResponse, HandshakeResponse } from '../protocol/response.js';
import {
  createHandshakeResponse as createBuiltinHandshakeResponse,
  handleBuiltinRequest,
} from './builtin-handlers.js';
import { handlePluginRequest } from './plugin-dispatch.js';

const registry = createPluginRegistry();

export async function createHandshakeResponse(
  request: HandshakeRequest,
): Promise<HandshakeResponse> {
  return createBuiltinHandshakeResponse(registry, request);
}

export async function handleRequest(request: BridgeRequest): Promise<BridgeResponse> {
  return (await handleExchange(request)).response;
}

export async function handleExchange(
  request: BridgeRequest,
  emit?: (event: BridgePluginEvent) => Promise<void>,
): Promise<BridgeExchange> {
  registry.metrics.recordRequest();
  const builtin = await handleBuiltinRequest(registry, request);
  return builtin ?? handlePluginRequest(registry, request, emit);
}

/**
 * ready 事件：bridge 启动后立即发出（原为同步读静态 builtinPlugins）。
 * 切片装配（§5.2 B.4）是懒加载的，因此这里先 await 装配完成再读快照，
 * 保证 pluginNames 与迁移前逐字节一致。
 */
export async function readyEvent(): Promise<BridgeEvent<{ plugins: string[] }>> {
  await registry.ready();
  const snapshot = registry.snapshot();
  return {
    method: 'event.ready',
    params: {
      plugins: snapshot.pluginNames,
    },
  };
}
