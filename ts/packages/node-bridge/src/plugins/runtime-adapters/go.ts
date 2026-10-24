import { resolveSpawnCommand } from './commands.js';
import { failureExecution, runSpawnBuild, startSpawnDev } from './spawn.js';
import type { RuntimeAdapter } from './types.js';

export const goRuntimeAdapter: RuntimeAdapter = {
  runtime: 'go',
  async handleDev(params, context) {
    const command = resolveSpawnCommand('go', 'dev', context, params);
    if (!command) {
      return failureExecution('go', 'dev', 'go dev command is not configured');
    }
    return await startSpawnDev('go', command, params.cwd);
  },
  async handleBuild(params, context) {
    if (params.watch) {
      return failureExecution('go', 'build', 'watch not supported for runtime go');
    }
    const command = resolveSpawnCommand('go', 'build', context, params);
    if (!command) {
      return failureExecution('go', 'build', 'go build command is not configured');
    }
    return await runSpawnBuild('go', command, params.cwd);
  },
};
