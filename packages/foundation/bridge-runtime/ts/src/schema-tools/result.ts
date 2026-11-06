/**
 * `tools.result`：帮助 schema 生成统一返回结构的小工具。
 *
 * 这层不涉及 host 调用，也没有策略校验。
 * 它存在的目的只是让成功/失败结果、附带事件和 exitCode 这几类常见包装方式更统一。
 */
import type { BridgeEvent } from '../protocol/events.js';

import { asRecord } from '../core/runtime.js';

export interface ResultTools {
  ok: (data?: unknown, options?: { exitCode?: number }) => Record<string, unknown>;
  fail: (
    message: string,
    options?: { exitCode?: number; data?: unknown },
  ) => Record<string, unknown>;
  withEvents: (
    result: unknown,
    events: BridgeEvent[],
  ) => { result: unknown; events: BridgeEvent[] };
  withExitCode: (result: unknown, exitCode: number) => Record<string, unknown>;
  event: (method: BridgeEvent['method'], params: Record<string, unknown>) => BridgeEvent;
}

export function createResultTools(): ResultTools {
  return {
    // `ok/fail` 只是约定俗成的返回形状，方便调用方和上层 bridge 更稳定地识别执行结果。
    ok: (data, options) => ({
      ok: true,
      exitCode: typeof options?.exitCode === 'number' ? options.exitCode : 0,
      ...(data === undefined ? {} : { data }),
    }),
    fail: (message, options) => ({
      ok: false,
      error: message,
      exitCode: typeof options?.exitCode === 'number' ? options.exitCode : 1,
      ...(options?.data === undefined ? {} : { data: options.data }),
    }),
    withEvents: (result, events) => ({ result, events }),
    withExitCode: (result, exitCode) => ({ ...asRecord(result), exitCode }),
    // 事件本身不做额外校验，这里只是省掉重复手写 `{ method, params }`。
    event: (method, params) => ({ method, params }),
  };
}
