import type { BridgeRequest } from '../protocol/request.js';

export function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

export function requestCwd(request: BridgeRequest): string | null {
  const params = asRecord(request.params);
  return typeof params.cwd === 'string' ? params.cwd : null;
}
