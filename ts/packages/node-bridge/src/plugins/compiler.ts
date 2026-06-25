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
  fallbackDevResult,
  findAvailablePort,
  resolveRuntimeWarning,
  type ActiveCompiler,
  type CompilerAdapterParams,
} from './compiler-shared.js';
import type { BridgeEvent } from '../protocol/events.js';
import { runCompilerInWorker } from './compiler-worker-client.js';
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
  // dev 端口如果被占用会自动向后探测，避免直接失败。
  const port = await findAvailablePort(requestedPort, host);
  const lanConfig = await loadLanConfig(cwd);
  const tool = detectBuildTool(lanConfig.config);
  const runtimeWarning = await resolveRuntimeWarning(cwd, tool, 'dev', lanConfig.config);

  // dev/run 一律走 worker：
  // - 避免把编译器依赖常驻在 bridge 主进程里（隔离崩溃/内存泄漏）
  // - 允许 stop 时精准停止该 worker（activeCompiler）
  const execution = await runCompilerInWorker('compiler.dev', createAdapterParams(cwd, host, port, params), {
    tool,
    lanConfig: lanConfig.config,
    runtimeWarning,
  });
  if (execution?.activeCompiler) {
    // activeCompiler 代表一个“可被 stop 的长任务”，供 compiler.stop 使用。
    activeCompiler = execution.activeCompiler;
  }
  // worker 未给出 execution 时走 fallback，保证协议层仍能给出可解释的结果与 warning。
  return execution ?? fallbackDevResult(tool, host, port, runtimeWarning ?? undefined);
}

async function handleBuild(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  const watch = params.watch === true;
  const lanConfig = await loadLanConfig(cwd);
  const tool = detectBuildTool(lanConfig.config);
  const mode = typeof params.mode === 'string' ? params.mode : null;
  const outputDir = typeof params.outputDir === 'string' ? params.outputDir : null;

  const runtimeWarning = await resolveRuntimeWarning(cwd, tool, 'build', lanConfig.config);
  // build/watch 与 dev 一样可能是长任务（watch=true），因此也会返回 activeCompiler。
  const execution = await runCompilerInWorker('compiler.build', createAdapterParams(cwd, '127.0.0.1', 0, params), {
    tool,
    lanConfig: lanConfig.config,
    runtimeWarning,
  });
  if (execution?.activeCompiler) {
    activeCompiler = execution.activeCompiler;
  }
  return execution ?? buildResult(tool, watch, mode, outputDir, 'fallback', runtimeWarning ?? undefined);
}

async function stopCompiler(reason: string) {
  const compiler = activeCompiler;
  activeCompiler = null;
  // stop 会先尝试停止 worker 内的编译器（如果存在），并把 worker 期间捕获的 events 追加到最终事件序列。
  const workerEvents = compiler ? (await compiler.stop()) ?? [] : [];

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
            compilerStatusEvent(compiler.tool, compiler.action, 'stopping', 'Stopping compiler runtime'),
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
