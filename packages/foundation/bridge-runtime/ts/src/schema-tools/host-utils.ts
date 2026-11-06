/**
 * `schema-tools` 内部复用的 host 调用小工具。
 *
 * 这个文件不直接暴露给 schema 作者，主要是给其它工具层复用：
 * - 统一补 `cwd`
 * - 提供一个更短的 `call()` 包装，减少重复的 `hostCall + result` 样板
 */
import { hostCall } from '../core/host-rpc.js';
import type { SchemaToolContext } from './types.js';

export function createHostInvoker(base: SchemaToolContext) {
  // 大部分 schema tool 都遵循“参数里没给 cwd 就继承 base.cwd”的规则，这里统一收口。
  const withCwd = (params?: Record<string, unknown>): Record<string, unknown> => ({
    ...(params ?? {}),
    cwd: (params?.cwd as string | undefined) ?? base.cwd,
  });

  const call = async <T = unknown>(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<T> => {
    // 内部 helper 默认只返回 `exchange.result`，适合绝大多数 facade 场景。
    const exchange = await hostCall<T>(method, withCwd(params));
    return exchange.result;
  };

  return {
    withCwd,
    call,
  };
}
