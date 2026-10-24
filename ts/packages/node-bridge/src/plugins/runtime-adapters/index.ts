import type { Runtime } from '../compiler-shared.js';
import { dartRuntimeAdapter } from './dart.js';
import { goRuntimeAdapter } from './go.js';
import { pythonRuntimeAdapter } from './python.js';
import { rustRuntimeAdapter } from './rust.js';
import type { RuntimeAdapter } from './types.js';

const runtimeAdapters: RuntimeAdapter[] = [
  goRuntimeAdapter,
  pythonRuntimeAdapter,
  rustRuntimeAdapter,
  dartRuntimeAdapter,
];

export function getRuntimeAdapters(): RuntimeAdapter[] {
  return runtimeAdapters.slice();
}

export function resolveRuntimeAdapter(runtime: Runtime): RuntimeAdapter | null {
  return runtimeAdapters.find((adapter) => adapter.runtime === runtime) ?? null;
}

export { createSpawnRuntimeAdapter } from './adapters.js';
export { resolveSpawnCommand } from './commands.js';
export { failureExecution, killProcessTree, runSpawnBuild, startSpawnDev } from './spawn.js';
export type { SpawnCommand } from './commands.js';
export type { RuntimeAdapter, SpawnRuntime } from './types.js';
