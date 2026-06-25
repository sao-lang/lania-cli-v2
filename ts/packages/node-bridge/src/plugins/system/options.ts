import { delimiter, resolve } from 'node:path';

import type { DiscoveredCommand, ListCommandOptions, ShellDiscoverySnapshot, CommandSource } from './types.js';

// 这一层只负责把外部请求参数归一化成稳定的 `ListCommandOptions`，
// 同时提供后续发现流程会复用的小型纯函数工具。
export function normalizeOptions(
  params: Record<string, unknown>,
  contextCwd?: string,
): ListCommandOptions {
  const cwd =
    typeof params.cwd === 'string' ? params.cwd : contextCwd && contextCwd.length ? contextCwd : process.cwd();
  const filter =
    typeof params.filter === 'string' && params.filter.trim().length > 0 ? params.filter.trim() : null;
  const limit =
    typeof params.limit === 'number' && Number.isFinite(params.limit) && params.limit > 0
      ? Math.trunc(params.limit)
      : null;
  const pathValue = typeof params.path === 'string' ? params.path : process.env.PATH ?? '';
  const shellExecutable =
    typeof params.shell === 'string' && params.shell.trim().length > 0
      ? params.shell.trim()
      : process.env.SHELL ?? null;

  return {
    cwd,
    filter,
    limit,
    allMatches: params.allMatches === true,
    includeShell: params.includeShell !== false,
    pathValue,
    shellExecutable,
  };
}

// notes 不是发现逻辑本身，而是给调用方解释“结果范围”的辅助说明。
// 放在这里是因为它只依赖选项与 shell 快照，不应混入 service 主流程。
export function buildNotes(includeShell: boolean, shellSnapshot: ShellDiscoverySnapshot): string[] {
  if (!includeShell) {
    return ['Only PATH-resolved executables are included because shell command discovery is disabled.'];
  }
  if (!shellSnapshot.executable) {
    return ['PATH executables are included; shell builtin/alias/function discovery is unavailable because SHELL is not set.'];
  }
  if (!shellSnapshot.supported) {
    return ['PATH executables are included; shell builtin/alias/function discovery currently supports zsh and bash only.'];
  }
  if (!shellSnapshot.loaded) {
    return ['PATH executables are included; shell builtin/alias/function discovery failed for the current shell session.'];
  }
  return ['Includes PATH executables plus shell builtins, aliases, and functions from the current shell.'];
}

// PATH 中可能出现相对路径、空白项和重复目录，这里统一归一化与去重。
// 这样 path-discovery 层只需关心“扫描哪些目录”，无需再处理输入清洗。
export function normalizePathEntries(pathValue: string, cwd: string): string[] {
  const seen = new Set<string>();
  const results: string[] = [];

  for (const entry of pathValue.split(delimiter)) {
    const trimmed = entry.trim();
    if (!trimmed) {
      continue;
    }
    const absolute = trimmed.startsWith('/') ? trimmed : resolve(cwd, trimmed);
    if (seen.has(absolute)) {
      continue;
    }
    seen.add(absolute);
    results.push(absolute);
  }

  return results;
}

// filter 匹配器也提前编译好，避免 service/path/shell 三处重复写大小写兼容逻辑。
export function createCommandMatcher(filter: string | null) {
  if (!filter) {
    return () => true;
  }
  const normalized = filter.toLowerCase();
  return (name: string) => name.toLowerCase().includes(normalized);
}

// 统一排序规则：先按命令名，再按来源优先级。
// 这样最终结果列表对调用方是稳定的，不会因为扫描顺序不同而波动。
export function compareCommands(left: DiscoveredCommand, right: DiscoveredCommand) {
  const nameOrder = left.name.localeCompare(right.name);
  if (nameOrder !== 0) {
    return nameOrder;
  }
  return sourceRank(left.source) - sourceRank(right.source);
}

function sourceRank(source: CommandSource) {
// PATH 命令通常是用户最直观理解的“可执行命令”，因此优先级最高；
// shell 内建、alias、function 依次排后，避免同名时结果顺序难以理解。
  switch (source) {
    case 'PATH':
    case 'PATH':
      return 0;
    case 'shell_builtin':
      return 1;
    case 'shell_alias':
      return 2;
    case 'shell_function':
      return 3;
  }
}
