/**
 * `tools.env`：为 schema/runtime 暴露一组只读环境信息。
 *
 * 这里没有策略校验，也不做环境变量写入。
 * 目标很简单：把当前执行上下文里最常用的环境读取能力做成稳定 API，
 * 例如 cwd、平台、CI 检测、home/tmpdir 和原始 env 访问。
 */
import { homedir, platform, tmpdir } from 'node:os';
import process from 'node:process';

export interface EnvTools {
  cwd: () => string;
  get: (name: string, defaultValue?: string) => string | undefined;
  has: (name: string) => boolean;
  all: () => Record<string, string | undefined>;
  platform: () => NodeJS.Platform;
  isCI: () => boolean;
  home: () => string;
  tmpdir: () => string;
}

export function createEnvTools(base: { cwd: string }): EnvTools {
  return {
    cwd: () => base.cwd,
    get: (name, defaultValue) => {
      const value = process.env[name];
      return value === undefined ? defaultValue : value;
    },
    has: (name) => process.env[name] !== undefined,
    all: () => ({ ...process.env }),
    platform: () => platform(),
    // CI 判断保持启发式，只覆盖当前工作流里最常见的 CI 环境标记。
    isCI: () =>
      process.env.CI === 'true' ||
      process.env.CI === '1' ||
      typeof process.env.GITHUB_ACTIONS === 'string' ||
      typeof process.env.BUILDKITE === 'string',
    home: () => homedir(),
    tmpdir: () => tmpdir(),
  };
}
