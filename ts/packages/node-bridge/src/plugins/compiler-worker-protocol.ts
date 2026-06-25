/**
 * 主进程与 compiler worker 之间的 IPC 消息协议。
 *
 * 主要导出：markWorkerExecution、CompilerWorkerMethod、CompilerWorkerRequest、CompilerWorkerMessage。
 *
 * 协议设计要点：
 * - worker 只响应两类请求：run（一次执行）与 stop（停止长任务）。
 * - run 的返回值总是 `result` 或 `error`：
 *   - `result.keepAlive=true` 表示 worker 内部保留了一个 activeCompiler（dev/watch），需要等待 stop。
 *   - `result.keepAlive=false` 表示短任务，worker 会在发送 result 后退出。
 * - `stopped` 是 stop 的确认信号：父进程可据此结束等待并回收 buffered events。
 *
 * markWorkerExecution 的作用：
 * - 把 workerMode/isolated 标记写入 result 和关键事件（start/done），让上游能区分“在主进程跑”还是“在隔离 worker 跑”。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type {
  BuildTool,
  CompilerAdapterContext,
  CompilerAdapterParams,
  CompilerExecution,
} from './compiler-shared.js';

export type CompilerWorkerMethod = 'compiler.dev' | 'compiler.build';

export type CompilerWorkerRequest =
  | {
      type: 'run';
      method: CompilerWorkerMethod;
      params: CompilerAdapterParams;
      context: CompilerAdapterContext;
    }
  | {
      // 仅对 keepAlive=true 的长任务有意义；短任务 worker 在 result 后已退出。
      type: 'stop';
    };

export type CompilerWorkerMessage =
  | {
      type: 'result';
      execution: Omit<CompilerExecution, 'activeCompiler'>;
      // keepAlive=true 表示 worker 将继续存活，等待后续 stop。
      keepAlive: boolean;
      tool: BuildTool;
    }
  | {
      type: 'stopped';
      tool: BuildTool | null;
    }
  | {
      type: 'error';
      error: {
        message: string;
        stack?: string;
      };
    };

export function markWorkerExecution(
  execution: Omit<CompilerExecution, 'activeCompiler'>,
): Omit<CompilerExecution, 'activeCompiler'> {
  return {
    ...execution,
    result: {
      ...execution.result,
      workerMode: 'isolated_worker',
      isolated: true,
    },
    events: execution.events.map((event) => {
      if (event.method === 'event.compiler_start' || event.method === 'event.compiler_done') {
        return {
          ...event,
          params: {
            ...(event.params as Record<string, unknown>),
            workerMode: 'isolated_worker',
            isolated: true,
          },
        } satisfies BridgeEvent;
      }
      return event;
    }),
  };
}
