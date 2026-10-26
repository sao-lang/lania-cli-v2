import { asRecord } from '../../core/runtime.js';
import type { CompilerAdapterContext, CompilerAdapterParams } from '../compiler-shared.js';
import { toolchainFor } from '../toolchain.js';
import type { SpawnRuntime } from './types.js';

export interface SpawnCommand {
  command: string;
  args: string[];
  env: Record<string, string>;
}

/**
 * 默认 build/dev 命令来自 `toolchain.ts`（唯一来源）。
 *
 * 优先级：`lan.config.runtimes[<id>][<action>]` 覆盖 > 内置命令。
 */
export function resolveSpawnCommand(
  runtime: SpawnRuntime,
  action: 'dev' | 'build',
  context: CompilerAdapterContext,
  params: CompilerAdapterParams,
): SpawnCommand | null {
  const configured = asRecord(asRecord(context.lanConfig.runtimes)[runtime])[action];
  const configuredArgs = Array.isArray(configured)
    ? configured.filter((value): value is string => typeof value === 'string')
    : [];
  const toolchain = toolchainFor(runtime);
  const builtin = action === 'dev' ? toolchain?.devCommand : toolchain?.buildCommand;
  const raw = configuredArgs.length > 0 ? configuredArgs : (builtin ?? []);
  if (!raw || raw.length === 0) {
    return null;
  }

  const output = params.outputDir ?? 'bin/app';
  const [command, ...rest] = raw.map((arg) => arg.replaceAll('{output}', output));
  const env: Record<string, string> =
    action === 'dev'
      ? { PORT: String(params.port), HOST: params.host }
      : { OUTPUT_DIR: params.outputDir ?? '' };
  return { command, args: rest, env };
}
