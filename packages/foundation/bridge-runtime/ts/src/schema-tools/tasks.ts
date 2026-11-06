/**
 * `tools.tasks`：面向宿主 task service 的任务编排 facade。
 *
 * 这一层既提供直接的 register/start/update/complete/fail/cancel API，
 * 也提供一个纯内存的 `TaskExecutor`，方便 schema 按顺序执行一组任务并自动把状态回写到 host。
 */
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface TaskTools {
  create: (options?: TaskExecutorOptions) => TaskExecutor;
  run: (tasks?: TaskExecutorTask[], options?: TaskExecutorOptions) => Promise<TaskExecutorResult[]>;
  register: (
    id: string,
    title: string,
    options?: { group?: string; priority?: 'high' | 'medium' | 'low' },
  ) => Promise<void>;
  start: (id: string, title?: string) => Promise<void>;
  update: (id: string, detail: string) => Promise<void>;
  complete: (id: string, detail?: string) => Promise<void>;
  fail: (id: string, detail?: string) => Promise<void>;
  cancel: (id: string, detail?: string) => Promise<void>;
  snapshot: () => Promise<{ tasks: unknown[] }>;
  events: () => Promise<{ events: unknown[] }>;
}

export interface TaskExecutorTask {
  id: string;
  title: string;
  group?: string;
  priority?: 'high' | 'medium' | 'low';
  detail?: string;
  run?: () => unknown | Promise<unknown>;
}

export interface TaskExecutorOptions {
  stopOnError?: boolean;
}

export interface TaskExecutorResult {
  id: string;
  status: 'completed' | 'failed' | 'cancelled';
  value?: unknown;
  error?: string;
}

export interface TaskExecutor {
  run: (tasks?: TaskExecutorTask[], options?: TaskExecutorOptions) => Promise<TaskExecutorResult[]>;
  add: (task: TaskExecutorTask) => TaskExecutor;
  addMany: (tasks: TaskExecutorTask[]) => TaskExecutor;
  pause: () => TaskExecutor;
  resume: () => TaskExecutor;
  cancel: () => TaskExecutor;
  cancelGroup: (group: string) => TaskExecutor;
  queue: () => TaskExecutorTask[];
  running: () => string[];
  results: () => TaskExecutorResult[];
  isPaused: () => boolean;
  isRunning: () => boolean;
}

export function createTaskTools(base: SchemaToolContext, policy: ToolsPolicyManager): TaskTools {
  const host = createHostInvoker(base);
  const createExecutor = (defaultOptions?: TaskExecutorOptions): TaskExecutor => {
    // executor 自己维护一套本地队列和结果缓存，host 只负责任务状态展示与持久化。
    const queue: TaskExecutorTask[] = [];
    const running = new Set<string>();
    const results: TaskExecutorResult[] = [];
    const registered = new Set<string>();
    const cancelledGroups = new Set<string>();
    let paused = false;
    let stopped = false;
    let active = false;
    let resumeResolver: (() => void) | null = null;

    const waitIfPaused = async () => {
      // pause/resume 通过一个简单的 promise 门闩实现，不引入额外调度线程。
      while (paused) {
        await new Promise<void>((resolve) => {
          resumeResolver = resolve;
        });
      }
    };

    const ensureRegistered = async (task: TaskExecutorTask) => {
      // 同一个 task id 只向 host 注册一次，避免重复出现在任务面板里。
      if (registered.has(task.id)) {
        return;
      }
      await host.call('host.tasks.register', {
        id: task.id,
        title: task.title,
        group: task.group ?? 'default',
        priority: task.priority ?? 'medium',
      });
      registered.add(task.id);
    };

    const cancelPendingTask = async (task: TaskExecutorTask) => {
      // stopOnError 或 cancel 时，尚未开始的任务统一按 cancelled 落盘。
      await ensureRegistered(task);
      await host.call('host.tasks.cancel', {
        id: task.id,
        detail: task.detail ?? 'cancelled',
      });
      results.push({ id: task.id, status: 'cancelled' });
    };

    const api: TaskExecutor = {
      run: async (tasks, runOptions) => {
        // executor 是串行执行模型，不做并发；重点是让状态推进和错误处理保持可预测。
        if (Array.isArray(tasks) && tasks.length > 0) {
          queue.push(...tasks);
        }
        const stopOnError = runOptions?.stopOnError ?? defaultOptions?.stopOnError ?? false;
        active = true;
        try {
          while (queue.length > 0) {
            await waitIfPaused();
            const task = queue.shift();
            if (!task) {
              continue;
            }
            if (stopped || (task.group && cancelledGroups.has(task.group))) {
              await cancelPendingTask(task);
              continue;
            }

            await ensureRegistered(task);
            await host.call('host.tasks.start', {
              id: task.id,
              title: task.title,
            });

            running.add(task.id);
            try {
              if (task.detail) {
                await host.call('host.tasks.update', {
                  id: task.id,
                  detail: task.detail,
                });
              }
              const value = task.run ? await task.run() : undefined;
              await host.call('host.tasks.complete', {
                id: task.id,
                detail: task.detail ?? 'done',
              });
              results.push({ id: task.id, status: 'completed', value });
            } catch (error) {
              const message = error instanceof Error ? error.message : String(error);
              await host.call('host.tasks.fail', {
                id: task.id,
                detail: message,
              });
              results.push({ id: task.id, status: 'failed', error: message });
              if (stopOnError) {
                stopped = true;
              }
            } finally {
              running.delete(task.id);
            }

            if (stopped) {
              while (queue.length > 0) {
                const pending = queue.shift();
                if (!pending) {
                  break;
                }
                await cancelPendingTask(pending);
              }
            }
          }
          return [...results];
        } finally {
          active = false;
        }
      },
      add: (task) => {
        queue.push(task);
        return api;
      },
      addMany: (tasks) => {
        queue.push(...tasks);
        return api;
      },
      pause: () => {
        paused = true;
        return api;
      },
      resume: () => {
        paused = false;
        resumeResolver?.();
        resumeResolver = null;
        return api;
      },
      cancel: () => {
        stopped = true;
        paused = false;
        resumeResolver?.();
        resumeResolver = null;
        return api;
      },
      cancelGroup: (group) => {
        cancelledGroups.add(group);
        return api;
      },
      queue: () => [...queue],
      running: () => [...running],
      results: () => [...results],
      isPaused: () => paused,
      isRunning: () => active,
    };

    return api;
  };

  return {
    create: (options) => createExecutor(options),
    run: async (tasks, options) => createExecutor(options).run(tasks, options),
    register: async (id, title, options) => {
      await policy.assertTasksAllowed('register');
      await host.call('host.tasks.register', {
        id,
        title,
        group: options?.group ?? 'default',
        priority: options?.priority ?? 'medium',
      });
    },
    start: async (id, title) => {
      await policy.assertTasksAllowed('start');
      await host.call('host.tasks.start', { id, title: title ?? id });
    },
    update: async (id, detail) => {
      await policy.assertTasksAllowed('update');
      await host.call('host.tasks.update', { id, detail });
    },
    complete: async (id, detail) => {
      await policy.assertTasksAllowed('complete');
      await host.call('host.tasks.complete', { id, detail: detail ?? 'done' });
    },
    fail: async (id, detail) => {
      await policy.assertTasksAllowed('fail');
      await host.call('host.tasks.fail', { id, detail: detail ?? 'failed' });
    },
    cancel: async (id, detail) => {
      await policy.assertTasksAllowed('cancel');
      await host.call('host.tasks.cancel', { id, detail: detail ?? 'cancelled' });
    },
    snapshot: async () => {
      await policy.assertTasksAllowed('snapshot');
      const result = await host.call<{ tasks: unknown[] }>('host.tasks.snapshot');
      return { tasks: Array.isArray(result.tasks) ? result.tasks : [] };
    },
    events: async () => {
      await policy.assertTasksAllowed('events');
      const result = await host.call<{ events: unknown[] }>('host.tasks.events');
      return { events: Array.isArray(result.events) ? result.events : [] };
    },
  };
}
