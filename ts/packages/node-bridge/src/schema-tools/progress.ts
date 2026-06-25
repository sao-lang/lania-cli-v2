/**
 * `tools.progress`：对宿主 progress service 的 schema facade。
 *
 * 它负责两类事：
 * - 把 begin/advance/message/detail/finish 这类进度更新动作转发到 host
 * - 在本地维护一组监听器，把 host summary 里的增量事件再分发给 schema 回调
 */
import { asRecord } from '../core/runtime.js';
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface ProgressTools {
  begin: (id: string, total?: number) => Promise<void>;
  beginGroup: (
    id: string,
    total?: number,
    kind?: 'spinner' | 'progress_bar' | 'static_step',
  ) => Promise<void>;
  beginStep: (
    id: string,
    parentId: string,
    total?: number,
    kind?: 'spinner' | 'progress_bar' | 'static_step',
  ) => Promise<void>;
  beginItem: (
    id: string,
    parentId: string,
    kind?: 'spinner' | 'progress_bar' | 'static_step',
  ) => Promise<void>;
  advance: (id: string, delta?: number) => Promise<void>;
  updateTotal: (id: string, total?: number) => Promise<void>;
  message: (id: string, message: string) => Promise<void>;
  detail: (id: string, detail: string) => Promise<void>;
  linkTask: (id: string, taskId: string) => Promise<void>;
  finish: (id: string) => Promise<void>;
  fail: (id: string, detail?: string) => Promise<void>;
  cancel: (id: string, detail?: string) => Promise<void>;
  reset: (id: string) => Promise<void>;
  resetAll: () => Promise<void>;
  completeAll: () => Promise<void>;
  failAll: (detail?: string) => Promise<void>;
  snapshot: () => Promise<{ items: unknown[] }>;
  events: () => Promise<{ events: unknown[] }>;
  summary: () => Promise<unknown>;
  render: (mode?: 'json' | 'indicatif') => Promise<string[]>;
  contains: (id: string) => Promise<boolean>;
  onProgress: (callback: (snapshot: unknown, event: unknown) => void | Promise<void>) => () => void;
  suspendTerminal: () => Promise<void>;
  resumeTerminal: () => Promise<void>;
}

export function createProgressTools(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
): ProgressTools {
  const host = createHostInvoker(base);
  const listeners = new Map<
    (snapshot: unknown, event: unknown) => void | Promise<void>,
    { sinceSequence: number }
  >();
  let lastSequence = 0;

  const dispatchProgressListeners = async () => {
    // 监听器并不是直接订阅 host 推送，而是每次变更后主动拉 summary，
    // 再根据 sequence 做一次本地增量分发。
    const summary = asRecord(
      await host.call<{ items?: unknown[]; events?: unknown[] }>('host.progress.summary'),
    );
    const items = Array.isArray(summary.items) ? summary.items : [];
    const events = Array.isArray(summary.events) ? summary.events : [];
    const snapshots = new Map<string, unknown>();
    for (const item of items) {
      const record = asRecord(item);
      const id =
        typeof record.id === 'string'
          ? record.id
          : typeof record.progress_id === 'string'
            ? record.progress_id
            : typeof record.progressId === 'string'
              ? record.progressId
              : null;
      if (id) {
        snapshots.set(id, item);
      }
    }
    const nextEvents = events.filter((event) => {
      const record = asRecord(event);
      const sequence = typeof record.sequence === 'number' ? record.sequence : 0;
      return sequence > lastSequence;
    });
    for (const event of nextEvents) {
      const record = asRecord(event);
      const sequence = typeof record.sequence === 'number' ? record.sequence : lastSequence;
      const progressId =
        typeof record.progress_id === 'string'
          ? record.progress_id
          : typeof record.progressId === 'string'
            ? record.progressId
            : null;
      const snapshot = progressId ? (snapshots.get(progressId) ?? null) : null;
      if (listeners.size > 0) {
        for (const [listener, state] of listeners) {
          if (sequence <= state.sinceSequence) {
            continue;
          }
          void listener(snapshot, event);
          state.sinceSequence = sequence;
        }
      }
      lastSequence = Math.max(lastSequence, sequence);
    }
  };

  const mutate = async (operation: string, action: () => Promise<void>) => {
    // 所有会改变进度状态的动作都统一走 `mutate()`，保证：
    // - 先做策略校验
    // - 再执行 host 调用
    // - 最后把新增事件广播给本地监听器
    await policy.assertProgressAllowed(operation);
    await action();
    await dispatchProgressListeners();
  };

  return {
    begin: async (id, total) =>
      mutate('begin', async () => {
        await host.call('host.progress.begin', { id, total });
      }),
    beginGroup: async (id, total, kind) =>
      mutate('beginGroup', async () => {
        await host.call('host.progress.beginGroup', { id, total, kind });
      }),
    beginStep: async (id, parentId, total, kind) =>
      mutate('beginStep', async () => {
        await host.call('host.progress.beginStep', { id, parentId, total, kind });
      }),
    beginItem: async (id, parentId, kind) =>
      mutate('beginItem', async () => {
        await host.call('host.progress.beginItem', { id, parentId, kind });
      }),
    advance: async (id, delta) =>
      mutate('advance', async () => {
        await host.call('host.progress.advance', { id, delta });
      }),
    updateTotal: async (id, total) =>
      mutate('updateTotal', async () => {
        await host.call('host.progress.updateTotal', { id, total });
      }),
    message: async (id, message) =>
      mutate('message', async () => {
        await host.call('host.progress.message', { id, message });
      }),
    detail: async (id, detail) =>
      mutate('detail', async () => {
        await host.call('host.progress.detail', { id, detail });
      }),
    linkTask: async (id, taskId) =>
      mutate('linkTask', async () => {
        await host.call('host.progress.linkTask', { id, taskId });
      }),
    finish: async (id) =>
      mutate('finish', async () => {
        await host.call('host.progress.finish', { id });
      }),
    fail: async (id, detail) =>
      mutate('fail', async () => {
        await host.call('host.progress.fail', { id, detail });
      }),
    cancel: async (id, detail) =>
      mutate('cancel', async () => {
        await host.call('host.progress.cancel', { id, detail });
      }),
    reset: async (id) =>
      mutate('reset', async () => {
        await host.call('host.progress.reset', { id });
      }),
    resetAll: async () =>
      mutate('resetAll', async () => {
        await host.call('host.progress.resetAll');
      }),
    completeAll: async () =>
      mutate('completeAll', async () => {
        await host.call('host.progress.completeAll');
      }),
    failAll: async (detail) =>
      mutate('failAll', async () => {
        await host.call('host.progress.failAll', { detail: detail ?? 'failed' });
      }),
    snapshot: async () => {
      await policy.assertProgressAllowed('snapshot');
      const result = await host.call<{ items: unknown[] }>('host.progress.snapshot');
      return { items: Array.isArray(result.items) ? result.items : [] };
    },
    events: async () => {
      await policy.assertProgressAllowed('events');
      const result = await host.call<{ events: unknown[] }>('host.progress.events');
      return { events: Array.isArray(result.events) ? result.events : [] };
    },
    summary: async () => {
      await policy.assertProgressAllowed('summary');
      return host.call('host.progress.summary');
    },
    render: async (mode) => {
      await policy.assertProgressAllowed('render');
      const result = await host.call<{ lines: string[] }>('host.progress.render', {
        mode: mode ?? 'indicatif',
      });
      return Array.isArray(result.lines) ? result.lines : [];
    },
    contains: async (id) => {
      await policy.assertProgressAllowed('contains');
      const result = await host.call<{ contains: boolean }>('host.progress.contains', {
        id,
      });
      return Boolean(result.contains);
    },
    onProgress: (callback) => {
      // 监听器默认从“当前最后一个 sequence 之后”开始接收事件，不追溯旧事件。
      listeners.set(callback, { sinceSequence: lastSequence });
      return () => {
        listeners.delete(callback);
      };
    },
    suspendTerminal: async () => {
      await policy.assertProgressAllowed('suspendTerminal');
      await host.call('host.progress.suspendTerminal');
    },
    resumeTerminal: async () => {
      await policy.assertProgressAllowed('resumeTerminal');
      await host.call('host.progress.resumeTerminal');
    },
  };
}
