import type { Runtime } from '../compiler-shared.js';
import { goRuntimeAdapter } from './go.js';
import type { RuntimeAdapter } from './types.js';

const runtimeAdapters: RuntimeAdapter[] = [goRuntimeAdapter];

export function getRuntimeAdapters(): RuntimeAdapter[] {
  return runtimeAdapters.slice();
}

export function resolveRuntimeAdapter(runtime: Runtime): RuntimeAdapter | null {
  return runtimeAdapters.find((adapter) => adapter.runtime === runtime) ?? null;
}

export { resolveSpawnCommand } from './commands.js';
export { failureExecution, killProcessTree, runSpawnBuild, startSpawnDev } from './spawn.js';
export type { SpawnCommand } from './commands.js';
export type { RuntimeAdapter, SpawnRuntime } from './types.js';
