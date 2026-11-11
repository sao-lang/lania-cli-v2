/**
 * Host RPC (Node -> Rust) transport helpers.
 *
 * Motivation:
 * - Schema handlers run inside the node-bridge process.
 * - Some tool namespaces must be host-backed (Rust), so Node needs to call back into the host.
 *
 * Transport:
 * - Node writes `{"type":"host_request","payload":{id,method,params}}` to stdout (jsonl).
 * - Rust replies on stdin with `{"type":"host_response","payload":{id,result?,error?,events}}`.
 *
 * Important:
 * - This module is transport-agnostic. `stdio.ts` installs the transport.
 */
import type { BridgeEvent } from '../protocol/events.js';

export type HostRequestPayload = {
  id: string;
  method: string;
  params: Record<string, unknown>;
};

export type HostResponsePayload = {
  id: string;
  result?: unknown;
  error?: { code: string; message: string; data?: unknown };
  events?: BridgeEvent[];
};

export type HostExchange<T = unknown> = {
  result: T;
  events: BridgeEvent[];
};

type HostOutgoingEnvelope = { type: 'host_request'; payload: HostRequestPayload };

type HostTransport = {
  write: (envelope: HostOutgoingEnvelope) => Promise<void>;
};

// 关键：src 与 dist 可能同时被加载（开发态 tsx 跑 src 入口，切片经包导出加载 dist），
// 模块级单例会分裂成两份 transport/pending 状态，导致一侧永远"transport not installed"。
// 这里把状态挂到 globalThis 上，保证同一进程内所有副本共享同一份 Host RPC 状态。
interface HostRpcGlobalState {
  transport: HostTransport | null;
  pending: Map<
    string,
    {
      resolve: (value: HostExchange) => void;
      reject: (error: Error) => void;
      timeout: NodeJS.Timeout | null;
    }
  >;
  sequence: number;
}

const HOST_RPC_STATE_KEY = '__lania_host_rpc_state__';

function hostRpcState(): HostRpcGlobalState {
  const holder = globalThis as unknown as Record<string, HostRpcGlobalState | undefined>;
  if (!holder[HOST_RPC_STATE_KEY]) {
    holder[HOST_RPC_STATE_KEY] = {
      transport: null,
      pending: new Map(),
      sequence: 0,
    };
  }
  return holder[HOST_RPC_STATE_KEY];
}

function nextId() {
  const state = hostRpcState();
  state.sequence += 1;
  return `host-req-${Date.now().toString(16)}-${state.sequence}`;
}

export function installHostRpcTransport(next: HostTransport) {
  hostRpcState().transport = next;
}

// Test helper: clears transport and pending requests.
export function resetHostRpcTransport() {
  const state = hostRpcState();
  state.transport = null;
  state.pending.clear();
}

export function handleHostResponse(payload: HostResponsePayload) {
  const { pending } = hostRpcState();
  const entry = pending.get(payload.id);
  if (!entry) return;
  pending.delete(payload.id);
  if (entry.timeout) {
    clearTimeout(entry.timeout);
  }
  if (payload.error) {
    entry.reject(new Error(`[${payload.error.code}] ${payload.error.message}`));
    return;
  }
  entry.resolve({
    result: payload.result,
    events: Array.isArray(payload.events) ? payload.events : [],
  });
}

export async function hostCall<T = unknown>(
  method: string,
  params: Record<string, unknown> = {},
  options?: { timeoutMs?: number },
): Promise<HostExchange<T>> {
  const { transport } = hostRpcState();
  if (!transport) {
    throw new Error('host rpc transport is not installed (stdio bridge not initialized)');
  }
  const id = nextId();
  const timeoutMs = typeof options?.timeoutMs === 'number' ? options.timeoutMs : 30_000;

  const { pending } = hostRpcState();
  const promise = new Promise<HostExchange<T>>((resolve, reject) => {
    const timeout =
      timeoutMs > 0
        ? setTimeout(() => {
            pending.delete(id);
            reject(new Error(`[E_HOST_TIMEOUT] host request timed out after ${timeoutMs}ms`));
          }, timeoutMs)
        : null;
    pending.set(id, { resolve: resolve as any, reject, timeout });
  });

  await transport.write({
    type: 'host_request',
    payload: { id, method, params },
  });

  return await promise;
}
