/**
 * bridge 插件接口定义，以及插件返回值的公共类型约束。
 *
 * 主要导出：BridgePluginResult、BridgePluginContext、BridgePlugin。
 */
import type { BridgeEvent } from '../protocol/events.js';

export interface BridgePluginResult<T = unknown> {
  result: T;
  events: BridgeEvent[];
}

export interface BridgePluginContext {
  cwd: string | null;
}

export interface BridgePlugin {
  name: string;
  methods: string[];
  handle(
    method: string,
    params: Record<string, unknown>,
    context?: BridgePluginContext,
  ): Promise<BridgePluginResult | null | undefined> | BridgePluginResult | null | undefined;
}
