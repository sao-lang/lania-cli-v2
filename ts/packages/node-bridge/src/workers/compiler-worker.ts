/**
 * compiler worker 进程入口，接收 run/stop 指令并托管 adapter 生命周期。
 *
 * 生命周期：
 * - 父进程发送 `run`：worker 执行一次 dev/build，并通过 IPC 回 `result` 或 `error`。
 * - 若执行结果包含 activeCompiler（dev/watch），worker 会 keepAlive=true 挂起；
 *   之后父进程发送 `stop`，worker 调用 activeCompiler.stop() 并回 `stopped` 后退出。
 *
 * 注意事项：
 * - worker 不应向 stdout 写入任意调试输出；父进程会捕获 stdout/stderr 并转换为 BridgeEvent，
 *   但如果未来 worker 被复用于 stdio 场景，stdout 污染会非常难排查。
 */
import type { BridgeEvent } from '../protocol/events.js';
import { resolveCompilerAdapter } from '../plugins/compiler-adapters/index.js';
import {
  buildResult,
  fallbackDevResult,
  type ActiveCompiler,
  type BuildTool,
} from '../plugins/compiler-shared.js';
import {
  markWorkerExecution,
  type CompilerWorkerMessage,
  type CompilerWorkerRequest,
} from '../plugins/compiler-worker-protocol.js';
import { loadLanConfig, normalizeError } from '../core/runtime.js';

let activeCompiler: ActiveCompiler | null = null;

process.on('message', async (message: CompilerWorkerRequest) => {
  // Worker 通过 IPC 接收父进程请求：
  // - run: 执行一次 dev/build，并把结果/keepAlive 通过 message 返回
  // - stop: 若存在 activeCompiler，则调用 stop 并退出
  // 注意：这里不要用 console.log 写 stdout，避免污染 stdio bridge 协议（父进程可能复用 stdout）。
  if (message.type === 'run') {
    await handleRun(message);
    return;
  }

  if (message.type === 'stop') {
    await handleStop();
  }
});

async function handleRun(message: Extract<CompilerWorkerRequest, { type: 'run' }>) {
  try {
    // Worker 自己加载 lan.config，避免把大对象通过 IPC 复制多次。
    const loadedLanConfig = await loadLanConfig(message.params.cwd);
    const context = {
      ...message.context,
      lanConfig: loadedLanConfig.config,
    };
    const adapter = resolveCompilerAdapter(message.context.tool);
    const execution =
      message.method === 'compiler.dev'
        ? await adapter?.handleDev(message.params, context)
        : await adapter?.handleBuild(message.params, context);

    // adapter 未提供实现时使用 fallback（用于在 worker 环境不可用时仍保持协议兼容）。
    const fallback = execution ?? createFallbackExecution(message);
    // 若执行结果携带 activeCompiler，说明是长任务（dev/watch），worker 需要保持存活。
    activeCompiler = fallback.activeCompiler ?? null;
    sendMessage({
      type: 'result',
      execution: markWorkerExecution(stripActiveCompiler(fallback)),
      keepAlive: Boolean(fallback.activeCompiler),
      tool: message.context.tool,
    });
    if (!fallback.activeCompiler) {
      // 非长任务：发送 result 后立即退出，减少僵尸进程概率。
      // 这里显式 exit 的原因：worker 进程不应长期挂着等待事件循环自然退出。
      process.exit(0);
    }
  } catch (error) {
    sendMessage({
      type: 'error',
      error: normalizeError(error),
    });
    // worker 侧失败直接退出 1：由父进程把错误转换成 bridge error（E_PLUGIN_RUNTIME 等）。
    process.exit(1);
  }
}

async function handleStop() {
  const compiler = activeCompiler;
  activeCompiler = null;
  if (compiler) {
    // stop 只对长任务有效；短任务 worker 在发送 result 后已 exit。
    await compiler.stop();
  }
  sendMessage({
    type: 'stopped',
    tool: compiler?.tool ?? null,
  });
  process.exit(0);
}

function createFallbackExecution(
  message: Extract<CompilerWorkerRequest, { type: 'run' }>,
) {
  return message.method === 'compiler.dev'
    ? fallbackDevResult(
        message.context.tool,
        message.params.host,
        message.params.port,
        message.context.runtimeWarning ?? undefined,
      )
    : buildResult(
        message.context.tool,
        message.params.watch,
        message.params.mode,
        message.params.outputDir,
        'fallback',
        message.context.runtimeWarning ?? undefined,
      );
}

function stripActiveCompiler<T extends { activeCompiler?: ActiveCompiler; events: BridgeEvent[] }>(
  execution: T,
) {
  const { activeCompiler: _activeCompiler, ...rest } = execution;
  return rest;
}

function sendMessage(message: CompilerWorkerMessage) {
  if (typeof process.send === 'function') {
    process.send(message);
  }
}
