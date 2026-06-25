import { access, lstat, readdir, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { extname, resolve } from 'node:path';

import type { DiscoveredCommand } from './types.js';

// 这一层只负责扫描 PATH 目录中的文件系统可执行项。
// 它不关心 filter、排序、limit 或 shell 内建命令，保持职责单一：
// 输入是一组已经归一化的目录，输出是扫描结果与重复项统计。
export async function discoverPathCommands(pathEntries: string[]) {
  const dedupedCommands = new Map<string, DiscoveredCommand>();
  const allMatches: DiscoveredCommand[] = [];
  const duplicatePaths = new Map<string, string[]>();
  let scannedDirs = 0;

  for (const directory of pathEntries) {
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true, encoding: 'utf8' });
      scannedDirs += 1;
    } catch {
      continue;
    }

    for (const entry of entries) {
      if (!(entry.isFile() || entry.isSymbolicLink())) {
        continue;
      }

      const entryName = String(entry.name);
      const absolutePath = resolve(directory, entryName);
      const kind = await resolveExecutableKind(absolutePath);
      if (!kind) {
        continue;
      }

      // `dedupedCommands` 保留同名命令的首个命中，模拟常见 shell 的 PATH 优先顺序；
      // `allMatches` 则保留完整命中列表，供 `allMatches=true` 时使用。
      const command: DiscoveredCommand = {
        name: entryName,
        source: 'PATH',
        kind,
        path: absolutePath,
        directory,
      };

      allMatches.push(command);
      if (!dedupedCommands.has(command.name)) {
        dedupedCommands.set(command.name, command);
        continue;
      }

      const firstPath = dedupedCommands.get(command.name)?.path;
      const paths = duplicatePaths.get(command.name) ?? (firstPath ? [firstPath] : []);
      if (command.path) {
        paths.push(command.path);
      }
      duplicatePaths.set(command.name, paths);
    }
  }

  return {
    dedupedCommands,
    allMatches,
    duplicatePaths,
    scannedDirs,
  };
}

// 可执行判断集中在这里，避免扫描主流程里混入平台细节。
// Unix 依赖 X_OK，Windows 依赖 PATHEXT 扩展名集合。
async function resolveExecutableKind(filePath: string): Promise<'file' | 'symlink' | null> {
  try {
    const linkStat = await lstat(filePath);
    const targetStat = linkStat.isSymbolicLink() ? await stat(filePath) : linkStat;
    if (!targetStat.isFile()) {
      return null;
    }

    if (process.platform === 'win32') {
      const extensions = (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM')
        .split(';')
        .map((item) => item.trim().toLowerCase())
        .filter(Boolean);
      const extension = extname(filePath).toLowerCase();
      if (extensions.length > 0 && !extensions.includes(extension)) {
        return null;
      }
      await access(filePath, constants.F_OK);
    } else {
      await access(filePath, constants.X_OK);
    }

    return linkStat.isSymbolicLink() ? 'symlink' : 'file';
  } catch {
    return null;
  }
}
