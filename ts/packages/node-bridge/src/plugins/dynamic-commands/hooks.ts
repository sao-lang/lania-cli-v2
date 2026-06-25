import { registerInlineHook } from '../../core/inline-hooks.js';
import { asRecord } from '../../core/runtime.js';
import { registerPendingInlineHook, takePendingInlineHook } from './state.js';
import type { HookBinding, HookBindings, InlineHookFn } from './types.js';

/**
 * 把 manifest 中的 hook 声明规范化成可序列化结构。
 * plugin hook 可直接序列化；inline function 先写入 pending registry，resolve 完成时再换成 final id。
 */
export function parseHookBindings(cwd: string, value: unknown): HookBindings | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const hooks: HookBindings = {};
  for (const [key, bindingValue] of Object.entries(record)) {
    const normalizedKey =
      key === 'preRun' ? 'onCommandPreInit' : key === 'postRun' ? 'onSuccess' : key;
    if (!Array.isArray(bindingValue)) {
      continue;
    }
    const bindings: HookBinding[] = [];
    for (const [index, entry] of bindingValue.entries()) {
      if (typeof entry === 'function') {
        const pendingId = `pending:${normalizedKey}:${index}:${Math.random().toString(16).slice(2)}`;
        registerPendingInlineHook(cwd, pendingId, entry as InlineHookFn);
        bindings.push({
          type: 'inline',
          kind: normalizedKey === 'onCommandPreInit' ? 'waterfall' : 'parallel',
          id: pendingId,
        });
        continue;
      }
      const binding = asRecord(entry);
      const plugin = typeof binding.plugin === 'string' ? binding.plugin : null;
      const handler = typeof binding.handler === 'string' ? binding.handler : null;
      if (!plugin || !handler) {
        continue;
      }
      bindings.push({
        type: 'plugin',
        kind:
          binding.kind === 'waterfall' || binding.kind === 'parallel' ? binding.kind : undefined,
        plugin,
        handler,
        timeoutMs:
          typeof binding.timeoutMs === 'number' && Number.isFinite(binding.timeoutMs)
            ? binding.timeoutMs
            : undefined,
        onError:
          binding.onError === 'throw' || binding.onError === 'collect'
            ? binding.onError
            : undefined,
      });
    }
    if (bindings.length > 0) {
      hooks[normalizedKey] = bindings;
    }
  }
  return Object.keys(hooks).length > 0 ? hooks : undefined;
}

export function mergeHooks(
  left: HookBindings | undefined,
  right: HookBindings | undefined,
): HookBindings | undefined {
  if (!left && !right) {
    return undefined;
  }
  const merged: HookBindings = {};
  for (const key of new Set([...Object.keys(left ?? {}), ...Object.keys(right ?? {})])) {
    const items = [...(left?.[key] ?? []), ...(right?.[key] ?? [])];
    if (items.length > 0) {
      merged[key] = items;
    }
  }
  return Object.keys(merged).length > 0 ? merged : undefined;
}

/**
 * resolve 完成前把 inline hook 的 pending id 固化成最终 id，供 Rust 在执行时回调。
 */
export function finalizeInlineHookBindings(
  cwd: string,
  handlerId: string,
  hooks: HookBindings | undefined,
): HookBindings | undefined {
  if (!hooks) {
    return undefined;
  }
  const result: HookBindings = {};
  for (const [hookKey, bindings] of Object.entries(hooks)) {
    result[hookKey] = bindings.map((binding, index) => {
      if (binding.type !== 'inline' || typeof binding.id !== 'string') {
        return binding;
      }
      const handler = takePendingInlineHook(cwd, binding.id);
      if (!handler) {
        return binding;
      }
      const finalId = `inline:${handlerId}:${hookKey}:${index}`;
      registerInlineHook(cwd, finalId, handler as InlineHookFn);
      return { ...binding, id: finalId };
    });
  }
  return result;
}

