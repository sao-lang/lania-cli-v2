/**
 * `tools.log`：把 schema 侧日志事件转发到宿主日志体系。
 *
 * 这层除了基础的 trace/debug/info/warn/error/success 之外，还提供：
 * - `scoped()`：为同一类日志自动补 target 前缀
 * - `entries()` / `clear()`：查看和清空当前日志缓冲
 * - `ascii()`：让 host 生成 ASCII 文本效果
 */
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface LogTools {
  log: (message: string, options?: { target?: string }) => Promise<void>;
  emit: (
    level: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'success',
    message: string,
    options?: { target?: string; traceId?: string; phase?: string; operation?: string },
  ) => Promise<void>;
  trace: (message: string, options?: { target?: string }) => Promise<void>;
  debug: (message: string, options?: { target?: string }) => Promise<void>;
  info: (message: string, options?: { target?: string }) => Promise<void>;
  warn: (message: string, options?: { target?: string }) => Promise<void>;
  error: (message: string, options?: { target?: string }) => Promise<void>;
  success: (message: string, options?: { target?: string }) => Promise<void>;
  scoped: (scope: string) => Omit<LogTools, 'scoped'>;
  entries: () => Promise<
    Array<{
      sequence: number;
      level: string;
      target: string;
      message: string;
      traceId?: string | null;
      phase?: string | null;
      operation?: string | null;
    }>
  >;
  clear: () => Promise<void>;
  ascii: (message: string) => Promise<string[]>;
}

export function createLogTools(base: SchemaToolContext, policy: ToolsPolicyManager): LogTools {
  const host = createHostInvoker(base);
  const normalizeLevel = (
    level: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'success',
  ): 'trace' | 'debug' | 'info' | 'warn' | 'error' => (level === 'success' ? 'info' : level);

  const emit: LogTools['emit'] = async (level, message, options) => {
    // `success` 在宿主日志级别里没有单独枚举，因此这里归一成 `info`。
    await policy.assertLogAllowed('emit');
    await host.call('host.log.emit', {
      level: normalizeLevel(level),
      message,
      target: options?.target ?? 'schema',
      traceId: options?.traceId ?? base.traceId ?? undefined,
      phase: options?.phase ?? 'event.log',
      operation: options?.operation ?? 'event.log',
    });
  };

  const createScoped = (scope?: string): Omit<LogTools, 'scoped'> => {
    // scoped logger 只是预先绑定 target，不会引入额外日志缓冲或隔离上下文。
    const withScope = (options?: { target?: string }) => ({
      target: options?.target ?? (scope ? `schema.${scope}` : 'schema'),
    });
    return {
      log: async (message, options) => emit('info', message, withScope(options)),
      emit: async (level, message, options) =>
        emit(level, message, {
          ...options,
          target: options?.target ?? (scope ? `schema.${scope}` : 'schema'),
        }),
      trace: async (message, options) => emit('trace', message, withScope(options)),
      debug: async (message, options) => emit('debug', message, withScope(options)),
      info: async (message, options) => emit('info', message, withScope(options)),
      warn: async (message, options) => emit('warn', message, withScope(options)),
      error: async (message, options) => emit('error', message, withScope(options)),
      success: async (message, options) => emit('success', message, withScope(options)),
      entries: async () => {
        await policy.assertLogAllowed('entries');
        const entries = await host.call<any[]>('host.log.entries');
        return scope ? entries.filter((entry) => entry?.target === `schema.${scope}`) : entries;
      },
      clear: async () => {
        await policy.assertLogAllowed('clear');
        await host.call('host.log.clear');
      },
      ascii: async (message) => {
        await policy.assertLogAllowed('ascii');
        const result = await host.call<{ lines: string[] }>('host.log.ascii', {
          message,
        });
        return Array.isArray(result.lines) ? result.lines : [];
      },
    };
  };

  return {
    ...createScoped(),
    emit,
    log: async (message, options) => emit('info', message, { target: options?.target }),
    trace: async (message, options) => emit('trace', message, options),
    debug: async (message, options) => emit('debug', message, options),
    info: async (message, options) => emit('info', message, options),
    warn: async (message, options) => emit('warn', message, options),
    error: async (message, options) => emit('error', message, options),
    success: async (message, options) => emit('success', message, options),
    scoped: (scope) => createScoped(scope),
    entries: async () => {
      await policy.assertLogAllowed('entries');
      const entries = await host.call<any[]>('host.log.entries');
      return Array.isArray(entries) ? entries : [];
    },
    clear: async () => {
      await policy.assertLogAllowed('clear');
      await host.call('host.log.clear');
    },
    ascii: async (message) => {
      await policy.assertLogAllowed('ascii');
      const result = await host.call<{ lines: string[] }>('host.log.ascii', {
        message,
      });
      return Array.isArray(result.lines) ? result.lines : [];
    },
  };
}
