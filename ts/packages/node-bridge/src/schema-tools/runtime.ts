/**
 * `tools.runtime`：脚本运行时的识别、计划与执行。
 *
 * Rust 侧的 `lania-runtime` 是唯一实现；这里只负责 host RPC 转发与策略校验。
 * 编译型语言的 prepare/cleanup 由 `host.runtime.run` 内部完成，Node 侧不拼装命令。
 */
import { hostCall } from '../core/host-rpc.js';
import type { ExecRunResult } from './exec.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface RuntimeCandidate {
  program: string;
  args: string[];
}

export interface RuntimeSpec {
  id: string;
  displayName: string;
  extensions: string[];
  candidates: RuntimeCandidate[];
  prepare: 'rustc' | 'cc' | null;
  packageManagers: string[];
}

export interface RuntimeCommand {
  program: string;
  args: string[];
  cwd?: string | null;
}

export interface RuntimeCommandPlan {
  runtime: string;
  language: string;
  program: string;
  args: string[];
  cwd?: string | null;
  preparedCommand?: RuntimeCommand | null;
  preparedSummary?: string | null;
}

export interface RuntimeRunResult extends ExecRunResult {
  file: string;
  runtime: string;
  language: string;
  program: string;
  args: string[];
  cwd: string;
  preparedCommand?: RuntimeCommand | null;
  preparedSummary?: string | null;
}

export interface RuntimeTools {
  list: () => Promise<RuntimeSpec[]>;
  detect: (
    file: string,
    cwd?: string,
  ) => Promise<{ file: string; language: string; runtime: string }>;
  spec: (language: string) => Promise<RuntimeSpec>;
  plan: (file: string, options?: { cwd?: string; args?: string[] }) => Promise<RuntimeCommandPlan>;
  run: (file: string, options?: { cwd?: string; args?: string[] }) => Promise<RuntimeRunResult>;
}

export function createRuntimeTools(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
): RuntimeTools {
  const call = async <T>(method: string, params: Record<string, unknown>): Promise<T> => {
    await policy.assertRuntimeAllowed(method);
    const exchange = await hostCall<T>(`host.runtime.${method}`, params);
    return exchange.result;
  };

  return {
    list: async () => {
      const result = await call<{ runtimes: RuntimeSpec[] }>('list', {});
      return Array.isArray(result.runtimes) ? result.runtimes : [];
    },
    detect: async (file, cwd) =>
      call<{ file: string; language: string; runtime: string }>('detect', {
        file,
        cwd: cwd ?? base.cwd,
      }),
    spec: async (language) => call<RuntimeSpec>('spec', { language }),
    plan: async (file, options) =>
      call<RuntimeCommandPlan>('plan', {
        file,
        cwd: options?.cwd ?? base.cwd,
        args: options?.args ?? [],
      }),
    run: async (file, options) =>
      call<RuntimeRunResult>('run', {
        file,
        cwd: options?.cwd ?? base.cwd,
        args: options?.args ?? [],
      }),
  };
}
