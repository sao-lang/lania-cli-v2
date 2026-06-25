/**
 * Inline Hooks Registry (Node-side only)
 *
 * 这是一套“仅存在于 Node 进程内”的 hook 注册/调用机制，用于承接以下场景：
 * - `lan.config.*` / `lania.schemas.*` 里允许用户直接写函数作为 hook/handler；
 * - Rust 端需要拿到一个“可序列化”的标识符（string id），随后通过 bridge 回调到 Node 执行真实函数。
 *
 * 关键约束：
 * - 这些函数无法跨进程序列化，因此 Rust 侧永远不会拿到函数体，只拿到 `id`。
 * - 该 registry 以 `cwd` 为隔离粒度（key = `${cwd}::${id}`），避免多项目/多次 resolve 互相污染。
 *
 * 返回约定：
 * - inline hook 执行结果既可以是任意值（作为新的 payload），也可以返回 `{ payload, events }` envelope。
 * - events 会以 `BridgeEvent[]` 形式回传给 Rust 端，用于日志/进度/诊断展示。
 *
 * 主要导出：clearInlineHooksForCwd、registerInlineHook、invokeInlineHook。
 */

import type { BridgeEvent } from '../protocol/events.js';
import { createSchemaTools, type SchemaTools } from './schema-tools.js';

type InlineHookFn = (
  payload: unknown,
  ctx: { cwd: string; hook: string; kind: string; source: string; tools: SchemaTools },
) => unknown | Promise<unknown>;

const INLINE_HOOKS = new Map<string, InlineHookFn>();

function keyFor(cwd: string, id: string) {
  return `${cwd}::${id}`;
}

/**
 * 清理某个 cwd 下注册的全部 inline hooks。
 *
 * 使用场景：
 * - 动态命令/manifest 每次 resolve 都会“重新编译”一份运行时命令树；
 * - 若不清理，旧的函数引用会泄漏并可能导致 handlerId 冲突或执行到旧逻辑。
 */
export function clearInlineHooksForCwd(cwd: string) {
  const prefix = `${cwd}::`;
  for (const key of INLINE_HOOKS.keys()) {
    if (key.startsWith(prefix)) {
      INLINE_HOOKS.delete(key);
    }
  }
}

/**
 * 注册一个 inline hook。
 *
 * 注意：
 * - `id` 是 Rust 侧持有的唯一引用；
 * - 同一个 cwd 下重复注册同 id 会覆盖旧值（符合“重新 resolve 覆盖旧定义”的直觉）。
 */
export function registerInlineHook(cwd: string, id: string, fn: InlineHookFn) {
  INLINE_HOOKS.set(keyFor(cwd, id), fn);
}

/**
 * 调用一个 inline hook，并把返回结果标准化为 `{ payload?, events }`。
 *
 * 兼容两种返回形态：
 * - 返回任意值：视为新的 payload，events 为空
 * - 返回 `{ payload, events }`：透传两者（events 必须是数组，否则降级为空数组）
 */
export async function invokeInlineHook(
  cwd: string,
  id: string,
  payload: unknown,
  ctx: { cwd: string; hook: string; kind: string; source: string },
): Promise<{ payload?: unknown; events: BridgeEvent[] }> {
  const fn = INLINE_HOOKS.get(keyFor(cwd, id));
  if (!fn) {
    throw new Error(`inline hook not found: ${id}`);
  }
  const toolEvents: BridgeEvent[] = [];
  const result = await fn(payload, {
    ...ctx,
    tools: createSchemaTools({
      cwd: ctx.cwd,
      traceId: null,
      hook: ctx.hook,
      hookKind: ctx.kind === 'waterfall' ? 'waterfall' : 'parallel',
      hookSource: ctx.source,
      events: toolEvents,
    }),
  });
  if (result && typeof result === 'object' && 'payload' in (result as Record<string, unknown>)) {
    const record = result as Record<string, unknown>;
    const events = Array.isArray(record.events) ? (record.events as BridgeEvent[]) : [];
    return { payload: record.payload, events: [...toolEvents, ...events] };
  }
  return { payload: result, events: toolEvents };
}
