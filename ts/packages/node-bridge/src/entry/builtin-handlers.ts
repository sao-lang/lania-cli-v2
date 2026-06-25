import { createPluginRegistry } from '../core/plugin-registry.js';
import type { BridgeExchange, HandshakeResponse } from '../protocol/response.js';
import type { BridgeRequest, HandshakeRequest } from '../protocol/request.js';
import {
  BRIDGE_EVENTS,
  BRIDGE_FAILURE_STRATEGY,
  BRIDGE_HEARTBEAT_INTERVAL_MS,
  BRIDGE_MAX_PENDING_EVENTS,
  BRIDGE_METHODS,
  BRIDGE_NAME,
} from './protocol-catalog.js';
import { requestCwd } from './request-context.js';

type PluginRegistry = ReturnType<typeof createPluginRegistry>;

export function createHandshakeResponse(
  registry: PluginRegistry,
  request: HandshakeRequest,
): HandshakeResponse {
  const snapshot = registry.snapshot();
  return {
    protocolVersion:
      request.protocolVersion ??
      (request as HandshakeRequest & { protocol_version?: string }).protocol_version ??
      '0.1.0',
    bridgeName: BRIDGE_NAME,
    methods: [...BRIDGE_METHODS, ...snapshot.methods],
    events: BRIDGE_EVENTS,
    heartbeatIntervalMs: BRIDGE_HEARTBEAT_INTERVAL_MS,
    maxPendingEvents: BRIDGE_MAX_PENDING_EVENTS,
    failureStrategy: BRIDGE_FAILURE_STRATEGY,
  };
}

export async function handleBuiltinRequest(
  registry: PluginRegistry,
  request: BridgeRequest,
): Promise<BridgeExchange | null> {
  switch (request.method) {
    case 'bridge.ping':
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            bridgeName: BRIDGE_NAME,
          },
        },
        events: [],
      };
    case 'bridge.heartbeat': {
      registry.metrics.recordHeartbeat();
      const ts = Date.now();
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            ts,
          },
        },
        events: [
          {
            method: 'event.heartbeat',
            params: { ts },
          },
        ],
      };
    }
    case 'bridge.metrics': {
      const cwd = requestCwd(request);
      const snapshot = registry.snapshot(cwd);
      return {
        response: {
          id: request.id,
          result: {
            ...registry.metrics.snapshot(),
            plugins: snapshot.pluginNames,
            rejectedPlugins: snapshot.rejectedPlugins,
          },
        },
        events: [],
      };
    }
    case 'plugins.resolve': {
      const cwd = requestCwd(request);
      const loaded = await registry.resolve(cwd);
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            cwd,
            plugins: loaded.pluginNames,
            methods: loaded.methods,
            rejectedPlugins: loaded.rejectedPlugins,
          },
        },
        events: [],
      };
    }
    case 'bridge.subscribe':
      return {
        response: {
          id: request.id,
          result: {
            accepted: true,
            events: BRIDGE_EVENTS,
            mode: 'request_response_stream',
          },
        },
        events: [],
      };
    case 'bridge.shutdown':
      return {
        response: {
          id: request.id,
          result: {
            accepted: true,
            stopped: true,
          },
        },
        events: [
          {
            method: 'event.shutdown',
            params: {
              reason: 'requested',
            },
          },
        ],
      };
    default:
      return null;
  }
}
