/**
 * compiler 插件入口，协调 dev、build、stop 请求并委派给独立 worker。
 *
 * 主要导出：compilerPlugin。
 */
import {
  buildResult,
  compilerDoneEvent,
  compilerStatusEvent,
  detectBuildTool,
  detectRuntime,
  fallbackDevResult,
  findAvailablePort,
  resolveRuntimeWarning,
  type ActiveCompiler,
  type CompilerAdapterParams,
  type Runtime,
} from './compiler-shared.js';
import type { BridgeEvent } from '../protocol/events.js';
import { runCompilerInWorker } from './compiler-worker-client.js';
import { failureExecution } from './runtime-adapters/spawn.js';
import { loadLanConfig } from '../core/runtime.js';

let activeCompiler: ActiveCompiler | null = null;

export const compilerPlugin = {
  name: 'compiler',
  methods: ['compiler.dev', 'compiler.build', 'compiler.stop'],
  async handle(method: string, params: Record<string, unknown>) {
    switch (method) {
      case 'compiler.dev':
        return handleDev(params);
      case 'compiler.build':
        return handleBuild(params);
      case 'compiler.stop':
        return stopCompiler('requested');
      default:
        return null;
    }
  },
};

async function handleDev(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  const host = typeof params.host === 'string' ? params.host : '127.0.0.1';
  const requestedPort = typeof params.port === 'number' ? params.port : 8089;
  let runtime: Runtime = 'web';
  try {
    const port = await findAvailablePort(requestedPort, host);
    const lanConfig = await loadLanConfig(cwd);
    runtime = await detectRuntime(lanConfig.config, cwd, params.runtime);
    const tool = detectBuildTool(lanConfig.config);
    // 非 web runtime 不读 package.json 依赖，也不伪造 dev server 警告。
    const runtimeWarning =
      runtime === 'web' ? await resolveRuntimeWarning(cwd, tool, 'dev', lanConfig.config) : null;

    const execution = await runCompilerInWorker(
      'compiler.dev',
      createAdapterParams(cwd, host, port, params),
      { tool, runtime, lanConfig: lanConfig.config, runtimeWarning },
    ).catch(() => undefined);
    if (execution?.activeCompiler) {
      activeCompiler = execution.activeCompiler;
    }
    if (execution) {
      return execution;
    }
    return runtime === 'web'
      ? fallbackDevResult(tool, host, port, runtimeWarning ?? undefined)
      : failureExecution(runtime, 'dev', 'runtime adapter unavailable');
  } catch {
    // web 保持既有兜底；非 web 必须 fail-fast，不能伪造可用的 dev server。
    return runtime === 'web'
      ? fallbackDevResult('vite', host, requestedPort, 'bridge worker unavailable')
      : failureExecution(runtime, 'dev', 'bridge worker unavailable');
  }
}

async function handleBuild(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  let runtime: Runtime = 'web';
  try {
    const watch = params.watch === true;
    const lanConfig = await loadLanConfig(cwd);
    runtime = await detectRuntime(lanConfig.config, cwd, params.runtime);
    const tool = detectBuildTool(lanConfig.config);
    const mode = typeof params.mode === 'string' ? params.mode : null;
    const outputDir = typeof params.outputDir === 'string' ? params.outputDir : null;
    const runtimeWarning =
      runtime === 'web' ? await resolveRuntimeWarning(cwd, tool, 'build', lanConfig.config) : null;

    const execution = await runCompilerInWorker(
      'compiler.build',
      createAdapterParams(cwd, '127.0.0.1', 0, params),
      { tool, runtime, lanConfig: lanConfig.config, runtimeWarning },
    ).catch(() => undefined);
    if (execution?.activeCompiler) {
      activeCompiler = execution.activeCompiler;
    }
    if (execution) {
      return execution;
    }
    return runtime === 'web'
      ? buildResult(tool, watch, mode, outputDir, 'fallback', runtimeWarning ?? undefined)
      : failureExecution(runtime, 'build', 'runtime adapter unavailable');
  } catch {
    return runtime === 'web'
      ? buildResult('vite', false, null, null, 'fallback', 'bridge worker unavailable')
      : failureExecution(runtime, 'build', 'bridge worker unavailable');
  }
}

async function stopCompiler(reason: string) {
  const compiler = activeCompiler;
  activeCompiler = null;
  // stop 会先尝试停止 worker 内的编译器（如果存在），并把 worker 期间捕获的 events 追加到最终事件序列。
  const workerEvents = compiler ? ((await compiler.stop()) ?? []) : [];

  return {
    result: {
      accepted: true,
      stopped: true,
    },
    events: [
      ...workerEvents,
      ...(compiler
        ? [
            // 这里补一组“可预期”的状态事件，便于 Rust 端统一映射到进度条与日志。
            compilerStatusEvent(
              compiler.tool,
              compiler.action,
              'stopping',
              'Stopping compiler runtime',
            ),
            compilerDoneEvent(compiler.tool, compiler.action, true, 'runtime', {
              stopped: true,
            }),
          ]
        : []),
      // shutdown 事件是“桥接层语义”：让 Rust 端知道 stop 已被处理并可以结束长任务等待。
      {
        method: 'event.shutdown',
        params: {
          reason,
        },
      } satisfies BridgeEvent,
    ],
  };
}

function createAdapterParams(
  cwd: string,
  host: string,
  port: number,
  params: Record<string, unknown>,
): CompilerAdapterParams {
  return {
    cwd,
    host,
    port,
    hmr: typeof params.hmr === 'boolean' ? params.hmr : null,
    open: params.open === true,
    watch: params.watch === true,
    mode: typeof params.mode === 'string' ? params.mode : null,
    outputDir: typeof params.outputDir === 'string' ? params.outputDir : null,
  };
}
