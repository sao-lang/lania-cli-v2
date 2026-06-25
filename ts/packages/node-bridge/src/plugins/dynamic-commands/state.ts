import { clearInlineHooksForCwd } from '../../core/inline-hooks.js';
import type { DynamicHandlerFn, InlineHookFn } from './types.js';

// Node-side function registries.
// Rust only holds serialized ids and always calls back into Node for actual execution.
const localExecutors = new Map<string, { handler: DynamicHandlerFn }>();
const pendingInlineHooks = new Map<string, InlineHookFn>();

function scopedKey(cwd: string, id: string): string {
  return `${cwd}::${id}`;
}

export function registerLocalExecutor(cwd: string, id: string, handler: DynamicHandlerFn) {
  localExecutors.set(scopedKey(cwd, id), { handler });
}

export function getLocalExecutor(cwd: string, id: string): DynamicHandlerFn | undefined {
  return localExecutors.get(scopedKey(cwd, id))?.handler;
}

export function registerPendingInlineHook(cwd: string, id: string, handler: InlineHookFn) {
  pendingInlineHooks.set(scopedKey(cwd, id), handler);
}

export function takePendingInlineHook(cwd: string, id: string): InlineHookFn | undefined {
  const key = scopedKey(cwd, id);
  const handler = pendingInlineHooks.get(key);
  if (handler) {
    pendingInlineHooks.delete(key);
  }
  return handler;
}

export function clearRegistriesForCwd(cwd: string) {
  const prefix = `${cwd}::`;
  for (const key of localExecutors.keys()) {
    if (key.startsWith(prefix)) {
      localExecutors.delete(key);
    }
  }
  for (const key of pendingInlineHooks.keys()) {
    if (key.startsWith(prefix)) {
      pendingInlineHooks.delete(key);
    }
  }
  clearInlineHooksForCwd(cwd);
}

