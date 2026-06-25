import { createPluginRegistry } from '../core/plugin-registry.js';
import type { BridgeExchange } from '../protocol/response.js';
import type { BridgeRequest } from '../protocol/request.js';
import { BRIDGE_METHODS } from './protocol-catalog.js';
import { asRecord, requestCwd } from './request-context.js';

type PluginRegistry = ReturnType<typeof createPluginRegistry>;

export async function handlePluginRequest(
  registry: PluginRegistry,
  request: BridgeRequest,
): Promise<BridgeExchange> {
  const cwd = requestCwd(request);
  const snapshot = await registry.resolve(cwd);
  if (![...BRIDGE_METHODS, ...snapshot.methods].includes(request.method)) {
    return {
      response: {
        id: request.id,
        error: {
          code: 'E_METHOD_NOT_FOUND',
          message: `Unsupported method: ${request.method}`,
        },
      },
      events: [],
    };
  }

  const plugin = await registry.resolvePlugin(request.method, cwd);
  try {
    const handled = await plugin?.handle(request.method, asRecord(request.params), { cwd });
    if (handled) {
      registry.metrics.recordEvents(handled.events.length);
      return {
        response: {
          id: request.id,
          result: handled.result,
        },
        events: handled.events,
      };
    }
  } catch (error) {
    registry.metrics.recordPluginError();
    return {
      response: {
        id: request.id,
        error: {
          code: 'E_PLUGIN_RUNTIME',
          message: error instanceof Error ? error.message : String(error),
        },
      },
      events: [],
    };
  }

  return {
    response: {
      id: request.id,
      result: {
        accepted: true,
        method: request.method,
        params: request.params,
      },
    },
    events: [],
  };
}
