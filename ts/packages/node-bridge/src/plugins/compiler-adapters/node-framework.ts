import type { BuildTool, CompilerAdapter } from '../compiler-shared.js';
import { resolvePackageBinaryFromCwd } from '../lint/shared.js';
import type { SpawnCommand } from '../runtime-adapters/commands.js';
import { failureExecution, runSpawnBuild, startSpawnDev } from '../runtime-adapters/spawn.js';

interface NodeFrameworkTool {
  packageName: string;
  binaryName: string;
  args: string[];
}

interface NodeFrameworkSpec {
  tool: Extract<BuildTool, 'nest' | 'tsc'>;
  dev: NodeFrameworkTool;
  build: NodeFrameworkTool;
}

/**
 * Node 框架（NestJS / Express+tsc）的构建工具适配器。
 * 输出统一走 runtime-adapters 的 compiler 事件与结果格式，和前端 build/dev 保持一致。
 */
export function createNodeFrameworkAdapter(spec: NodeFrameworkSpec): CompilerAdapter {
  return {
    tool: spec.tool,
    async handleDev(params) {
      const binary = resolvePackageBinaryFromCwd(
        params.cwd,
        spec.dev.packageName,
        spec.dev.binaryName,
      );
      if (!binary) {
        return failureExecution(
          spec.tool,
          'dev',
          `${spec.dev.packageName} is not installed; run the package manager install first`,
        );
      }
      const command: SpawnCommand = {
        command: binary,
        args: spec.dev.args,
        env: { PORT: String(params.port), HOST: params.host },
      };
      return await startSpawnDev(spec.tool, command, params.cwd);
    },
    async handleBuild(params) {
      if (params.watch) {
        return failureExecution(spec.tool, 'build', `watch not supported for tool ${spec.tool}`);
      }
      const binary = resolvePackageBinaryFromCwd(
        params.cwd,
        spec.build.packageName,
        spec.build.binaryName,
      );
      if (!binary) {
        return failureExecution(
          spec.tool,
          'build',
          `${spec.build.packageName} is not installed; run the package manager install first`,
        );
      }
      const command: SpawnCommand = {
        command: binary,
        args: spec.build.args,
        env: { OUTPUT_DIR: params.outputDir ?? '' },
      };
      return await runSpawnBuild(spec.tool, command, params.cwd);
    },
  };
}

export const nestCompilerAdapter = createNodeFrameworkAdapter({
  tool: 'nest',
  dev: { packageName: '@nestjs/cli', binaryName: 'nest', args: ['start', '--watch'] },
  build: { packageName: '@nestjs/cli', binaryName: 'nest', args: ['build'] },
});

export const tscCompilerAdapter = createNodeFrameworkAdapter({
  tool: 'tsc',
  dev: { packageName: 'tsx', binaryName: 'tsx', args: ['watch', 'src/index.ts'] },
  build: { packageName: 'typescript', binaryName: 'tsc', args: ['-p', 'tsconfig.json'] },
});
