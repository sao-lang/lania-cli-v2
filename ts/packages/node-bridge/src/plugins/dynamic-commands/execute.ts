import type { BridgePluginResult } from '../../core/bridge-plugin.js';
import { asRecord } from '../../core/runtime.js';
import { prepareDynamicCommandExecution, asExitCode } from './execute-context.js';
import { runDynamicExecutor } from './execute-runner.js';
import type { DynamicInvocationTarget } from './types.js';

/**
 * 执行动态命令。
 * summary target 只返回帮助信息；manifest_command 才会真正调用本地函数或插件方法。
 */
export async function invokeDynamicCommand(
  cwd: string,
  params: Record<string, unknown>,
): Promise<BridgePluginResult<Record<string, unknown>>> {
  const target = asRecord(params.target) as DynamicInvocationTarget;

  if (!target.kind) {
    throw new Error('dynamic command target is required');
  }

  if (target.kind === 'mount_summary') {
    return {
      result: {
        kind: 'dynamic_mount_summary',
        mount: target.mount,
        commands: target.commands,
        exitCode: 0,
      },
      events: [],
    };
  }

  if (target.kind === 'group_summary') {
    return {
      result: {
        kind: 'dynamic_group_summary',
        mount: target.mount,
        path: target.path,
        commands: target.commands,
        exitCode: 0,
      },
      events: [],
    };
  }

  if (target.kind !== 'manifest_command') {
    throw new Error(`unsupported dynamic command target kind: ${String((target as { kind?: unknown }).kind)}`);
  }

  const traceId = typeof params.traceId === 'string' ? params.traceId : null;
  const { context, toolEvents } = await prepareDynamicCommandExecution(cwd, params, target);

  try {
    const execution = await runDynamicExecutor(cwd, target, context);
    return {
      result: {
        kind: 'dynamic_manifest_command_result',
        mount: target.mount,
        path: target.path,
        executor: target.executor,
        result: execution.result,
        exitCode: asExitCode(execution.result),
      },
      events: [...toolEvents, ...execution.events],
    };
  } catch (error) {
    const err = error instanceof Error ? { message: error.message } : { message: String(error) };
    return {
      result: {
        kind: 'dynamic_manifest_command_result',
        mount: target.mount,
        path: target.path,
        executor: target.executor,
        result: { error: err.message, exitCode: 1 },
        exitCode: 1,
      },
      events: toolEvents,
    };
  }
}
