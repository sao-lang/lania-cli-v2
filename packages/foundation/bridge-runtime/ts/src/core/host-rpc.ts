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

let transport: HostTransport | null = null;
const pending = new Map<
  string,
  {
    resolve: (value: HostExchange) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout | null;
  }
>();

let sequence = 0;
function nextId() {
  sequence += 1;
  return `host-req-${Date.now().toString(16)}-${sequence}`;
}

export function installHostRpcTransport(next: HostTransport) {
  transport = next;
}

// Test helper: clears transport and pending requests.
export function resetHostRpcTransport() {
  transport = null;
  pending.clear();
}

export function handleHostResponse(payload: HostResponsePayload) {
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
  if (!transport) {
    throw new Error('host rpc transport is not installed (stdio bridge not initialized)');
  }
  const id = nextId();
  const timeoutMs = typeof options?.timeoutMs === 'number' ? options.timeoutMs : 30_000;

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
