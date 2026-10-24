/** 与宿主 bridge 之间的结构化约定（刻意不 import node-bridge，避免依赖反转）。 */

export interface EmitEvent {
  method: string;
  params: unknown;
}

/** `hostCall` 的返回形状，与 node-bridge 的 `HostExchange` 对齐。 */
export interface HostExchangeLike<T = unknown> {
  result: T;
  events?: EmitEvent[];
}

export type HostCall = <T = unknown>(
  method: string,
  params?: Record<string, unknown>,
  options?: { timeoutMs?: number },
) => Promise<HostExchangeLike<T>>;

export interface AgentPluginContext {
  cwd: string | null;
  emit?: (event: EmitEvent) => Promise<void>;
  hostCall?: HostCall;
}

export interface AgentPluginResult {
  result: unknown;
  events: EmitEvent[];
}

export interface AgentPlugin {
  name: string;
  methods: string[];
  handle(
    method: string,
    params: Record<string, unknown>,
    context?: AgentPluginContext,
  ): Promise<AgentPluginResult | undefined>;
}
