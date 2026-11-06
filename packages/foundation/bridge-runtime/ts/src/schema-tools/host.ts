/**
 * `tools.host`：对任意 host rpc 方法的受控透传入口。
 *
 * 和其它 schema tools 不同，这一层不提供业务语义化方法，
 * 而是保留一个足够通用的 `call()`，让 schema 在确实需要时可以直接访问宿主能力。
 *
 * 关键约束：
 * - 每次调用都要先过 `assertHostCallAllowed`
 * - 默认自动注入 `cwd`
 * - 同步把当前 tools policy 里的 exec/fs 子策略传给 host，保持边界一致
 */
import { hostCall, type HostExchange } from '../core/host-rpc.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export type { HostExchange } from '../core/host-rpc.js';

export interface HostTools {
  call: <T = unknown>(
    method: string,
    params?: Record<string, unknown>,
    options?: { timeoutMs?: number },
  ) => Promise<HostExchange<T>>;
}

export function createHostTools(base: SchemaToolContext, policy: ToolsPolicyManager): HostTools {
  return {
    call: async <T>(
      method: string,
      params?: Record<string, unknown>,
      options?: { timeoutMs?: number },
    ) => {
      await policy.assertHostCallAllowed(method);
      // host 侧也需要知道当前 tools policy，尤其是 exec/fs 这类高风险能力的子策略。
      const resolvedPolicy = await policy.get();
      return hostCall<T>(
        method,
        {
          ...(params ?? {}),
          cwd: (params ?? {}).cwd ?? base.cwd,
          __toolsPolicy: {
            exec: resolvedPolicy.exec,
            fs: resolvedPolicy.fs,
          },
        },
        options,
      );
    },
  };
}
