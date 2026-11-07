/**
 * 核侧本地 bridge 分发注册表（阶段 4b 拆包的依赖倒置点）。
 *
 * 主要导出：registerLocalBridgeHandler、dispatchLocalBridgeMethod、dispatchBuiltinBridge。
 *
 * 背景：原 `schema-tools/bridge.ts` 的 `dispatchBuiltinBridge` 静态 import 了
 * config/compiler/lint/system/commitizen/commitlint 六个插件实例，导致插件无法
 * 拆出独立切片包。现在改为：
 * - 核只认「方法前缀 → handler」注册表；
 * - `core/slices.ts` 的 `loadSlices()` 在加载每个切片后，把该切片 plugin.methods
 *   的前缀注册进来（切片保持被动，无模块副作用）；
 * - 切片未加载（不在 `LANIA_BRIDGE_SLICES` 清单）时其方法自然不可用，
 *   与动态插件的可用性语义一致。
 */

import type { BridgeEvent } from '../protocol/events.js';
import type { BridgeExchange, BridgeResponse } from '../protocol/response.js';

export interface LocalBridgeContext {
  cwd: string | null;
}

export interface LocalBridgeHandlerResult {
  result: unknown;
  events?: BridgeEvent[];
}

export type LocalBridgeHandler = (
  method: string,
  params: Record<string, unknown>,
  context: LocalBridgeContext,
) => Promise<LocalBridgeHandlerResult | null>;

interface RegisteredHandler {
  prefixes: string[];
  handle: LocalBridgeHandler;
}

const handlers: RegisteredHandler[] = [];

/**
 * 把一个已加载插件的全部方法前缀注册进本地分发表。
 * 前缀取每个 method 的第一段（如 `config.loadLan` → `config.`）。
 * 重复注册同一 handler 幂等（按引用去重）。
 */
export function registerLocalBridgeHandler(
  handle: LocalBridgeHandler,
  methods: readonly string[],
): void {
  if (handlers.some((entry) => entry.handle === handle)) {
    return;
  }
  const prefixes = [
    ...new Set(
      methods
        .filter((method) => typeof method === 'string' && method.includes('.'))
        .map((method) => `${method.split('.')[0]}.`),
    ),
  ];
  handlers.push({ prefixes, handle });
}

/** 供测试隔离使用：清空注册表（仅测试场景调用）。 */
export function resetLocalBridgeHandlers(): void {
  handlers.length = 0;
}

/**
 * 本地内建方法分发。没有注册过的前缀直接返回 null，由调用方决定
 * 上抛 unsupported 还是交给更外层桥接。
 */
export async function dispatchLocalBridgeMethod(
  method: string,
  params: Record<string, unknown>,
  cwd: string,
): Promise<{ response: BridgeResponse; events: BridgeEvent[] } | null> {
  const context: LocalBridgeContext = { cwd };
  for (const entry of handlers) {
    if (!entry.prefixes.some((prefix) => method.startsWith(prefix))) {
      continue;
    }
    const handled = await entry.handle(method, params, context);
    if (handled) {
      const response: BridgeResponse = {
        id: `local-${Math.random().toString(16).slice(2)}`,
        result: handled.result,
      };
      return { response, events: handled.events ?? [] };
    }
  }
  return null;
}

/**
 * 兼容入口：保持原 `schema-tools/bridge.ts::dispatchBuiltinBridge` 的语义——
 * 未命中时抛 `unsupported bridge method in schema tools`。
 */
export async function dispatchBuiltinBridge(
  method: string,
  params: Record<string, unknown>,
  cwd: string,
): Promise<BridgeExchange> {
  const handled = await dispatchLocalBridgeMethod(method, params, cwd);
  if (!handled) {
    throw new Error(`unsupported bridge method in schema tools: ${method}`);
  }
  return { response: handled.response, events: handled.events };
}
