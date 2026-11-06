/**
 * bridge 插件接口定义，以及插件返回值的公共类型约束。
 *
 * 主要导出：BridgePluginResult、BridgePluginContext、BridgePlugin。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type { hostCall } from '../core/host-rpc.js';

/** 插件可见的事件形状：method 用 string，避免内置插件与外置插件互相绑死枚举。 */
export interface BridgePluginEvent {
  method: string;
  params: unknown;
}

export interface BridgePluginResult<T = unknown> {
  result: T;
  events: BridgeEvent[];
}

export interface BridgePluginContext {
  cwd: string | null;
  /**
   * 立即写出一条流式事件。
   *
   * 普通插件把事件放在返回值里即可；只有需要在请求结束前推送增量（例如 agent 的 token）
   * 时才使用。写入顺序由 stdio 的串行化保证，`response` 仍然最后发出。
   */
  emit?: (event: BridgePluginEvent) => Promise<void>;
  /** 反向调用宿主能力（Rust）。语义等同 `hostCall`，由 stdio 注入。 */
  hostCall?: typeof hostCall;
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
