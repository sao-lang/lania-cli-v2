import type { BridgeEvent } from '../protocol/events.js';

export type HookKind = 'parallel' | 'waterfall';

export interface SchemaToolContext {
  cwd: string;
  traceId: string | null;
  mount?: string;
  path?: string[];
  commandHandlerId?: string;
  hook?: string;
  hookKind?: HookKind;
  hookSource?: string;
  events?: BridgeEvent[];
}
