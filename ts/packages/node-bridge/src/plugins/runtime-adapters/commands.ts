import { asRecord } from '../../core/runtime.js';
import type { CompilerAdapterContext, CompilerAdapterParams } from '../compiler-shared.js';
import type { SpawnRuntime } from './types.js';

export interface SpawnCommand {
  command: string;
  args: string[];
  env: Record<string, string>;
}

const DEFAULT_COMMANDS: Record<SpawnRuntime, { dev?: string[]; build?: string[] }> = {
  go: { dev: ['go', 'run', '.'], build: ['go', 'build', '-o', '{output}', '.'] },
  rust: { dev: ['cargo', 'run'], build: ['cargo', 'build', '--release'] },
  python: { build: ['python', '-m', 'build'] },
  dart: { dev: ['dart', 'run'], build: ['dart', 'compile', 'exe', 'bin/main.dart'] },
};

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
  const raw = configuredArgs.length > 0 ? configuredArgs : DEFAULT_COMMANDS[runtime][action];
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
