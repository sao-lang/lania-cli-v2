import { discoverPathCommands } from './path-discovery.js';
import {
  buildNotes,
  compareCommands,
  createCommandMatcher,
  normalizePathEntries,
} from './options.js';
import {
  createEmptyShellSnapshot,
  discoverShellCommands,
  flattenShellCommands,
} from './shell-discovery.js';
import type { ListCommandOptions } from './types.js';

// service 层负责把 PATH 扫描、shell 探测、过滤、排序、摘要统计整合成最终响应。
// 各个子模块只产出局部能力，真正对外暴露的“系统命令发现结果”在这里收口。
export async function discoverSystemCommands(options: ListCommandOptions) {
  const pathEntries = normalizePathEntries(options.pathValue, options.cwd);
  const { dedupedCommands, allMatches, duplicatePaths, scannedDirs } =
    await discoverPathCommands(pathEntries);

  // matcher 在这里统一应用到 PATH 唯一结果、PATH 全量结果和 shell 结果上，
  // 保证不同来源的筛选语义完全一致。
  const matcher = createCommandMatcher(options.filter);
  const filteredUnique = [...dedupedCommands.values()].filter((command) => matcher(command.name));
  const filteredAllMatches = allMatches.filter((command) => matcher(command.name));
  const shellSnapshot = options.includeShell
    ? await discoverShellCommands(options.shellExecutable, options.cwd)
    : createEmptyShellSnapshot(options.shellExecutable);
  const filteredShellCommands = flattenShellCommands(shellSnapshot).filter((command) =>
    matcher(command.name),
  );
  // `allMatches=false` 时模拟用户平时使用命令时看到的“首个命中”视图；
  // `allMatches=true` 时则暴露 PATH 中所有同名可执行项，便于排查遮蔽问题。
  const selectedPathCommands = options.allMatches ? filteredAllMatches : filteredUnique;
  const selected = [...selectedPathCommands, ...filteredShellCommands];
  const sorted = selected.sort(compareCommands);
  const limited = options.limit === null ? sorted : sorted.slice(0, options.limit);

  return {
    accepted: true,
    kind: 'system_commands',
    scope: 'environment',
    platform: process.platform,
    shell: shellSnapshot.executable,
    shellName: shellSnapshot.shellName,
    shellSupported: shellSnapshot.supported,
    includeShell: options.includeShell,
    cwd: options.cwd,
    filter: options.filter,
    limit: options.limit,
    allMatches: options.allMatches,
    notes: buildNotes(options.includeShell, shellSnapshot),
    summary: {
      pathEntries: pathEntries.length,
      scannedDirs,
      unique: filteredUnique.length,
      duplicates: [...duplicatePaths.keys()].filter((name) => matcher(name)).length,
      shellBuiltins: filteredShellCommands.filter((command) => command.source === 'shell_builtin').length,
      shellAliases: filteredShellCommands.filter((command) => command.source === 'shell_alias').length,
      shellFunctions: filteredShellCommands.filter((command) => command.source === 'shell_function').length,
      matched: selected.length,
      returned: limited.length,
    },
    commands: limited,
    duplicates: [...duplicatePaths.entries()]
      .filter(([name]) => matcher(name))
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([name, paths]) => ({ name, paths })),
  };
}
