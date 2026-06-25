/**
 * `tools.workspace`：面向当前工作区的轻量查询工具。
 *
 * 这里关注的是“工作区长什么样”，而不是执行命令：
 * - 当前 cwd/root 是什么
 * - 有没有某个文件
 * - git 根在哪
 * - package manager 更像哪一种
 * - lan/tool 配置能否被读到
 */
import { readFile } from 'node:fs/promises';
import path from 'node:path';

import {
  asRecord,
  fileExists,
  findFirstExisting,
  loadLanConfig,
  loadToolConfig,
} from '../core/runtime.js';

export interface WorkspaceTools {
  cwd: () => string;
  root: () => string;
  gitRoot: () => Promise<string | null>;
  packageJson: (cwd?: string) => Promise<Record<string, unknown> | null>;
  hasFile: (filePath: string) => Promise<boolean>;
  findFile: (candidates: string[], cwd?: string) => Promise<string | null>;
  detectPackageManager: (cwd?: string) => Promise<'pnpm' | 'yarn' | 'npm' | 'unknown'>;
  loadLanConfig: (cwd?: string) => Promise<unknown>;
  loadToolConfig: (tool: string, cwd?: string) => Promise<unknown>;
}

export function createWorkspaceTools(
  base: { cwd: string },
  policy: { assertWorkspaceAllowed: (operation: string) => Promise<void> },
): WorkspaceTools {
  return {
    cwd: () => base.cwd,
    root: () => base.cwd,
    gitRoot: async () => {
      await policy.assertWorkspaceAllowed('gitRoot');
      return findGitRoot(base.cwd);
    },
    packageJson: async (cwd) => {
      await policy.assertWorkspaceAllowed('packageJson');
      return readPackageJson(cwd ?? base.cwd);
    },
    hasFile: async (filePath) => {
      await policy.assertWorkspaceAllowed('hasFile');
      return fileExists(path.isAbsolute(filePath) ? filePath : path.resolve(base.cwd, filePath));
    },
    findFile: async (candidates, cwd) => {
      await policy.assertWorkspaceAllowed('findFile');
      return findFirstExisting(cwd ?? base.cwd, candidates);
    },
    detectPackageManager: async (cwd) => {
      await policy.assertWorkspaceAllowed('detectPackageManager');
      return detectPackageManager(cwd ?? base.cwd);
    },
    loadLanConfig: async (cwd) => {
      await policy.assertWorkspaceAllowed('loadLanConfig');
      return loadLanConfig(cwd ?? base.cwd);
    },
    loadToolConfig: async (tool, cwd) => {
      await policy.assertWorkspaceAllowed('loadToolConfig');
      return loadToolConfig(cwd ?? base.cwd, tool);
    },
  };
}

async function findGitRoot(start: string): Promise<string | null> {
  // 从当前目录一路向上找 `.git`，直到文件系统根。
  let current = path.resolve(start);
  while (true) {
    const candidate = path.join(current, '.git');
    if (await fileExists(candidate)) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

async function readPackageJson(cwd: string): Promise<Record<string, unknown> | null> {
  // package.json 解析失败时按“没有可用 package.json”处理，避免把低层 JSON 错误直接抛给调用方。
  const filePath = path.resolve(cwd, 'package.json');
  if (!(await fileExists(filePath))) {
    return null;
  }
  try {
    return asRecord(JSON.parse(await readFile(filePath, 'utf8')));
  } catch {
    return null;
  }
}

async function detectPackageManager(cwd: string): Promise<'pnpm' | 'yarn' | 'npm' | 'unknown'> {
  // 这里只按 lockfile 启发式判断，足够应对 schema 层的大多数分支选择。
  if (await fileExists(path.resolve(cwd, 'pnpm-lock.yaml'))) return 'pnpm';
  if (await fileExists(path.resolve(cwd, 'yarn.lock'))) return 'yarn';
  if (await fileExists(path.resolve(cwd, 'package-lock.json'))) return 'npm';
  return 'unknown';
}
