/**
 * 编译 worker 客户端，启动独立进程并把 IPC/输出桥接成事件。
 *
 * 主要导出：runCompilerInWorker。
 * 关键点：
 * - 包含子进程/IPC 交互
 * - 包含文件系统读写/路径解析
 * - 包含 stdio 协议/流式读写
 *
 * 设计说明（为什么要 worker）：
 * - 编译器依赖栈通常很重（vite/webpack/rollup + 插件），直接挂在 bridge 主进程里会放大内存泄漏与崩溃影响面。
 * - 通过独立进程隔离后，dev/watch 可以把生命周期绑定到 worker：需要停时发送 stop，worker 自己退出。
 *
 * 事件合并策略：
 * - worker stdout/stderr 会被转成 BridgeEvent 并先缓存在 bufferedEvents 中。
 * - 最终返回给上层时会合并为：`[...bufferedEvents, ...execution.events]`（保证“先输出日志，再输出结构化事件”）。
 */
import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import readline from 'node:readline';

import type { BridgeEvent } from '../protocol/events.js';
import {
  compilerIssueEvent,
  logEvent,
  type BuildTool,
  type CompilerAdapterContext,
  type CompilerAdapterParams,
  type CompilerExecution,
} from './compiler-shared.js';
import type {
  CompilerWorkerMessage,
  CompilerWorkerMethod,
  CompilerWorkerRequest,
} from './compiler-worker-protocol.js';

const workerEntrypoint = resolveWorkerEntrypoint();

export async function runCompilerInWorker(
  method: CompilerWorkerMethod,
  params: CompilerAdapterParams,
  context: CompilerAdapterContext,
): Promise<CompilerExecution> {
  // 独立进程执行：
  // - 隔离编译器运行时（崩溃不会带崩 bridge 主进程）
  // - 便于在 dev/watch 时把 activeCompiler 生命周期绑定到 worker
  const child = spawn(process.execPath, ['--import', 'tsx', workerEntrypoint], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    stdio: ['ignore', 'pipe', 'pipe', 'ipc'],
  });

  const bufferedEvents: BridgeEvent[] = [];
  // 将 worker 的 stdout/stderr 转成 bridge events 缓存起来，最后并入 execution.events。
  attachStreamCapture(child.stdout, context.tool, 'stdout', bufferedEvents);
  attachStreamCapture(child.stderr, context.tool, 'stderr', bufferedEvents);

  return await new Promise<CompilerExecution>((resolve, reject) => {
    let settled = false;
    let resultMessage: Extract<CompilerWorkerMessage, { type: 'result' }> | null = null;

    child.once('error', (error) => {
      if (!settled) {
        settled = true;
        reject(error);
      }
    });

    child.on('message', (message: CompilerWorkerMessage) => {
      if (message.type === 'error' && !settled) {
        settled = true;
        reject(new Error(message.error.message));
        return;
      }

      if (message.type === 'result' && !settled) {
        // worker 在 run 完成后会先发 result；
        // - 短任务（build / dev non-watch）随后退出
        // - 长任务（dev/watch）会 keepAlive=true 挂起等待 stop
        resultMessage = message;
        if (message.keepAlive) {
          // keepAlive=true 表示 worker 内部存在 activeCompiler（dev/watch），
          // 需要返回一个可调用的 stop()，由 Rust 端在 Ctrl-C 时触发。
          settled = true;
          resolve({
            ...message.execution,
            events: [...bufferedEvents, ...message.execution.events],
            activeCompiler: {
              tool: message.tool,
              action:
                message.execution.result.action === 'dev' ? 'dev' : 'build',
              stop: async () => stopCompilerWorker(child, message.tool, bufferedEvents),
            },
          });
        }
      }
    });

    child.once('exit', (code, signal) => {
      if (settled) {
        return;
      }
      if (resultMessage) {
        // 非长任务：result 先到，随后 worker 自己退出。
        settled = true;
        resolve({
          ...resultMessage.execution,
          events: [...bufferedEvents, ...resultMessage.execution.events],
        });
        return;
      }
      settled = true;
      reject(
        new Error(
          `compiler worker exited before returning a result (code=${code ?? 'null'}, signal=${signal ?? 'null'})`,
        ),
      );
    });

    const request: CompilerWorkerRequest = {
      type: 'run',
      method,
      params,
      context,
    };
    child.send(request);
  });
}

function resolveWorkerEntrypoint() {
  const compiledWorker = fileURLToPath(
    new URL('../workers/compiler-worker.js', import.meta.url),
  );
  if (existsSync(compiledWorker)) {
    return compiledWorker;
  }
  return fileURLToPath(new URL('../workers/compiler-worker.ts', import.meta.url));
}

async function stopCompilerWorker(
  child: ReturnType<typeof spawn>,
  tool: BuildTool,
  bufferedEvents: BridgeEvent[],
): Promise<BridgeEvent[]> {
  // stop 的语义：
  // - 只在 keepAlive=true 的长任务下有意义（短任务 worker 已 exit）
  // - 尽力发送 `{ type: 'stop' }`，然后等待 worker 回 `stopped` 或直接 exit
  // - 无论哪条路径结束，都把 bufferedEvents drain 出来返回给上层，作为 stop() 的事件补齐
  if (!child.connected) {
    // IPC 已断开：无法发送 stop 请求，只能把已捕获的 events 交还。
    return drainWorkerEvents(bufferedEvents);
  }

  return await new Promise<BridgeEvent[]>((resolve, reject) => {
    let finished = false;

    const cleanup = () => {
      child.off('message', onMessage);
      child.off('error', onError);
      child.off('exit', onExit);
    };

    const finish = (events: BridgeEvent[]) => {
      if (finished) {
        return;
      }
      finished = true;
      cleanup();
      resolve(events);
    };

    const onMessage = (message: CompilerWorkerMessage) => {
      if (message.type === 'stopped') {
        finish(drainWorkerEvents(bufferedEvents));
      } else if (message.type === 'error') {
        finished = true;
        cleanup();
        reject(new Error(message.error.message));
      }
    };

    const onError = (error: Error) => {
      finished = true;
      cleanup();
      reject(error);
    };

    const onExit = () => {
      // 无论 stop 是否成功，exit 都视作 worker 生命周期结束。
      finish(drainWorkerEvents(bufferedEvents));
    };

    child.on('message', onMessage);
    child.once('error', onError);
    child.once('exit', onExit);
    child.send({ type: 'stop' } satisfies CompilerWorkerRequest);
    if (!child.connected) {
      finish(drainWorkerEvents(bufferedEvents));
      return;
    }

    bufferedEvents.push(
      logEvent(`[${tool}:worker] stop requested`, 'info'),
    );
  });
}

function attachStreamCapture(
  stream: NodeJS.ReadableStream | null,
  tool: BuildTool,
  source: 'stdout' | 'stderr',
  bufferedEvents: BridgeEvent[],
) {
  if (!stream) {
    return;
  }
  const rl = readline.createInterface({
    input: stream,
    crlfDelay: Infinity,
  });
  rl.on('line', (line) => {
    const text = line.trim();
    if (!text) {
      return;
    }
    bufferedEvents.push(...workerOutputEvents(tool, source, text));
  });
}

function workerOutputEvents(
  tool: BuildTool,
  source: 'stdout' | 'stderr',
  text: string,
): BridgeEvent[] {
  // 约定：
  // - stderr 一律视作“至少是 warning”，同时也作为 log 事件回传，便于用户侧排查
  // - stdout 仅作为 info 日志回传，不做结构化解析（避免引入对编译器输出格式的耦合）
  if (source === 'stderr') {
    return [
      compilerIssueEvent(tool, 'warning', `[worker stderr] ${text}`),
      logEvent(`[${tool}:worker:${source}] ${text}`, 'warn'),
    ];
  }
  return [logEvent(`[${tool}:worker:${source}] ${text}`, 'info')];
}

function drainWorkerEvents(bufferedEvents: BridgeEvent[]) {
  const events = bufferedEvents.slice();
  bufferedEvents.length = 0;
  return events;
}
