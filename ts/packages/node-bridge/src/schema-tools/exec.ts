/**
 * `tools.exec`：schema 侧执行外部命令的统一入口。
 *
 * 它同时提供三层能力：
 * - builder 风格的命令拼装（`command()` / `shell()`）
 * - 一次性执行（`run` / `runChecked` / `spawn` / `bash`）
 * - 历史查询
 *
 * 真正的进程执行都桥接回 host，这样 timeout、环境变量策略、审计和 kill 行为
 * 都能和 Rust 侧执行模型保持一致。
 */
import { hostCall } from '../core/host-rpc.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface ExecTools {
  workingDir: () => string;
  command: (program: string) => ExecCommandBuilder;
  shell: (script: string) => ExecCommandBuilder;
  run: (command: ExecCommandInput | ExecCommandBuilder) => Promise<ExecRunResult>;
  runWithOptions: (
    command: ExecCommandInput | ExecCommandBuilder,
    options?: ExecRunInvokeOptions,
  ) => Promise<ExecRunResult>;
  runChecked: (command: ExecCommandInput | ExecCommandBuilder) => Promise<ExecRunResult>;
  runCheckedWithOptions: (
    command: ExecCommandInput | ExecCommandBuilder,
    options?: ExecRunInvokeOptions,
  ) => Promise<ExecRunResult>;
  history: () => Promise<
    Array<{
      program: string;
      args: string[];
      cwd?: string | null;
      env?: Record<string, string>;
      use_shell?: boolean;
      useShell?: boolean;
    }>
  >;
  bash: (
    script: string,
    options?: { cwd?: string; timeoutMs?: number; checked?: boolean },
  ) => Promise<{ exitCode: number; stdout: string; stderr: string }>;
  bashChecked: (
    script: string,
    options?: { cwd?: string; timeoutMs?: number },
  ) => Promise<{ exitCode: number; stdout: string; stderr: string }>;
  spawn: (
    program: string,
    args?: string[],
    options?: Omit<ExecRunInvokeOptions, 'useShell'>,
  ) => Promise<ExecRunResult>;
  spawnChecked: (
    program: string,
    args?: string[],
    options?: Omit<ExecRunInvokeOptions, 'useShell'>,
  ) => Promise<ExecRunResult>;
}

export interface ExecCommandInput {
  program: string;
  args?: string[];
  cwd?: string;
  env?: Record<string, string>;
  useShell?: boolean;
}

export interface ExecRunInvokeOptions {
  cwd?: string;
  env?: Record<string, string>;
  timeoutMs?: number;
  killProcessTree?: boolean;
  useShell?: boolean;
}

export interface ExecRunResult {
  exitCode: number;
  stdout: string;
  stderr: string;
  skipped?: boolean;
  timedOut?: boolean;
  cancelled?: boolean;
}

export interface ExecCommandBuilder {
  program: string;
  args: string[];
  cwd?: string;
  env: Record<string, string>;
  useShell: boolean;
  withArgs: (args: string[]) => ExecCommandBuilder;
  inDir: (cwd: string) => ExecCommandBuilder;
  withEnv: (key: string, value: string) => ExecCommandBuilder;
  run: (options?: ExecRunInvokeOptions) => Promise<ExecRunResult>;
  runChecked: (options?: ExecRunInvokeOptions) => Promise<ExecRunResult>;
}

export function createExecTools(base: SchemaToolContext, policy: ToolsPolicyManager): ExecTools {
  // 先把 builder/input 两种输入形式收敛成统一命令结构，后面所有执行路径都复用它。
  const normalizeExecCommand = (
    command: ExecCommandInput | ExecCommandBuilder,
  ): ExecCommandInput => ({
    program: command.program,
    args: [...(command.args ?? [])],
    cwd: command.cwd,
    env: { ...(command.env ?? {}) },
    useShell: command.useShell ?? false,
  });

  const mergeExecCommand = (
    command: ExecCommandInput | ExecCommandBuilder,
    options?: ExecRunInvokeOptions,
  ): ExecCommandInput &
    Required<Pick<ExecRunInvokeOptions, 'killProcessTree'>> & { timeoutMs?: number } => {
    // 最终执行前在这里合并默认 cwd、调用时 env 覆盖以及 shell/timeout 选项。
    const normalized = normalizeExecCommand(command);
    return {
      program: normalized.program,
      args: normalized.args ?? [],
      cwd: options?.cwd ?? normalized.cwd ?? base.cwd,
      env: { ...(normalized.env ?? {}), ...(options?.env ?? {}) },
      useShell: options?.useShell ?? normalized.useShell ?? false,
      timeoutMs: options?.timeoutMs,
      killProcessTree: options?.killProcessTree ?? false,
    };
  };

  const runViaHost = async (
    method: 'host.exec.run' | 'host.exec.runChecked',
    operation: string,
    command: ExecCommandInput | ExecCommandBuilder,
    options?: ExecRunInvokeOptions,
  ): Promise<ExecRunResult> => {
    // 所有实际执行都统一经过 host，避免不同调用路径出现策略/日志口径不一致。
    const merged = mergeExecCommand(command, options);
    await policy.assertExecAllowed(operation, { useShell: merged.useShell, env: merged.env });
    const exchange = await hostCall<ExecRunResult>(method, {
      program: merged.program,
      args: merged.args ?? [],
      cwd: merged.cwd ?? base.cwd,
      env: merged.env ?? {},
      timeoutMs: merged.timeoutMs,
      killProcessTree: merged.killProcessTree,
      useShell: merged.useShell,
    });
    return exchange.result;
  };

  const createBuilder = (command: ExecCommandInput): ExecCommandBuilder => {
    // builder 是不可变快照风格：每次 `withArgs/inDir/withEnv` 都返回一个新 builder。
    const snapshot = normalizeExecCommand(command);
    return {
      program: snapshot.program,
      args: snapshot.args ?? [],
      cwd: snapshot.cwd,
      env: snapshot.env ?? {},
      useShell: snapshot.useShell ?? false,
      withArgs: (args) => createBuilder({ ...snapshot, args: [...(snapshot.args ?? []), ...args] }),
      inDir: (cwd) => createBuilder({ ...snapshot, cwd }),
      withEnv: (key, value) =>
        createBuilder({
          ...snapshot,
          env: { ...(snapshot.env ?? {}), [key]: value },
        }),
      run: async (options) => runViaHost('host.exec.run', 'run', snapshot, options),
      runChecked: async (options) =>
        runViaHost('host.exec.runChecked', 'runChecked', snapshot, options),
    };
  };

  return {
    workingDir: () => base.cwd,
    command: (program) => createBuilder({ program, args: [], env: {}, useShell: false }),
    shell: (script) => createBuilder({ program: script, args: [], env: {}, useShell: true }),
    run: async (command) => runViaHost('host.exec.run', 'run', command),
    runWithOptions: async (command, options) =>
      runViaHost('host.exec.run', 'runWithOptions', command, options),
    runChecked: async (command) => runViaHost('host.exec.runChecked', 'runChecked', command),
    runCheckedWithOptions: async (command, options) =>
      runViaHost('host.exec.runChecked', 'runCheckedWithOptions', command, options),
    history: async () => {
      await policy.assertExecAllowed('history');
      const exchange = await hostCall<any[]>('host.exec.history', { cwd: base.cwd });
      return Array.isArray(exchange.result) ? exchange.result : [];
    },
    bash: async (script, options) => {
      await policy.assertExecAllowed('bash', { useShell: true });
      const exchange = await hostCall<any>('host.exec.shell', {
        cwd: options?.cwd ?? base.cwd,
        script,
        timeoutMs: options?.timeoutMs,
        checked: options?.checked ?? false,
      });
      return exchange.result as { exitCode: number; stdout: string; stderr: string };
    },
    bashChecked: async (script, options) => {
      await policy.assertExecAllowed('bashChecked', { useShell: true });
      const exchange = await hostCall<any>('host.exec.shell', {
        cwd: options?.cwd ?? base.cwd,
        script,
        timeoutMs: options?.timeoutMs,
        checked: true,
      });
      return exchange.result as { exitCode: number; stdout: string; stderr: string };
    },
    spawn: async (program, args, options) =>
      runViaHost('host.exec.run', 'spawn', { program, args: args ?? [], useShell: false }, options),
    spawnChecked: async (program, args, options) =>
      runViaHost(
        'host.exec.runChecked',
        'spawnChecked',
        { program, args: args ?? [], useShell: false },
        options,
      ),
  };
}
