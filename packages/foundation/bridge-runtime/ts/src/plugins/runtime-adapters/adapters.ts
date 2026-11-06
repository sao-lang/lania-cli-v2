import { resolveSpawnCommand } from './commands.js';
import { failureExecution, runSpawnBuild, startSpawnDev } from './spawn.js';
import type { RuntimeAdapter, SpawnRuntime } from './types.js';

/**
 * 所有 spawn 型 runtime 的 dev/build 行为一致：解析命令 -> 拒绝不支持的 watch -> spawn。
 * 差异只在默认命令表与错误文案里的 runtime 名，因此收敛到工厂，避免四份复制。
 */
export function createSpawnRuntimeAdapter(runtime: SpawnRuntime): RuntimeAdapter {
  return {
    runtime,
    async handleDev(params, context, emit) {
      const command = resolveSpawnCommand(runtime, 'dev', context, params);
      if (!command) {
        return failureExecution(runtime, 'dev', `${runtime} dev command is not configured`, emit);
      }
      return await startSpawnDev(runtime, command, params.cwd, emit);
    },
    async handleBuild(params, context, emit) {
      if (params.watch) {
        return failureExecution(
          runtime,
          'build',
          `watch not supported for runtime ${runtime}`,
          emit,
        );
      }
      const command = resolveSpawnCommand(runtime, 'build', context, params);
      if (!command) {
        return failureExecution(
          runtime,
          'build',
          `${runtime} build command is not configured`,
          emit,
        );
      }
      return await runSpawnBuild(runtime, command, params.cwd, emit);
    },
  };
}
