/**
 * `tools.fs`：文件系统读写 facade。
 *
 * 这个模块同时混合了两类实现：
 * - host-backed 操作：read/write/remove/readdir/stat 等，需要统一策略校验和宿主视角路径
 * - 本地直接操作：copy/move/glob 这类在 Node 侧更便宜或更容易实现的能力
 *
 * 关键约束是：
 * - 所有写操作都必须先过 `assertFsWriteAllowed`
 * - 相对路径默认都以 `base.cwd` 解析
 * - 返回值尽量保持简单稳定，避免把宿主内部细节泄漏给 schema
 */
import {
  copyFile,
  mkdir as mkdirFs,
  readdir as readdirFs,
  rename as renameFs,
  stat as statFs,
} from 'node:fs/promises';
import path from 'node:path';

import { hostCall } from '../core/host-rpc.js';
import { fileExists } from '../core/runtime.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

export interface FsTools {
  exists: (filePath: string) => Promise<boolean>;
  read: (filePath: string) => Promise<string>;
  readJson: <T = unknown>(filePath: string) => Promise<T>;
  append: (filePath: string, content: string, options?: { mkdirp?: boolean }) => Promise<void>;
  write: (
    filePath: string,
    content: string,
    options?: { append?: boolean; mkdirp?: boolean },
  ) => Promise<void>;
  writeJson: (
    filePath: string,
    value: unknown,
    options?: { space?: number; mkdirp?: boolean },
  ) => Promise<void>;
  mkdir: (dirPath: string, options?: { recursive?: boolean }) => Promise<void>;
  mkdirp: (dirPath: string) => Promise<void>;
  remove: (filePath: string, options?: { recursive?: boolean }) => Promise<{ removed: boolean }>;
  copy: (from: string, to: string, options?: { overwrite?: boolean }) => Promise<void>;
  move: (from: string, to: string, options?: { overwrite?: boolean }) => Promise<void>;
  readdir: (dirPath: string) => Promise<string[]>;
  stat: (
    filePath: string,
  ) => Promise<{ isFile: boolean; isDir: boolean; size: number; mtimeMs?: number }>;
  glob: (
    pattern: string,
    options?: { cwd?: string; absolute?: boolean; onlyFiles?: boolean },
  ) => Promise<string[]>;
  ensureFile: (filePath: string, content?: string) => Promise<void>;
  ensureDir: (dirPath: string) => Promise<void>;
  replace: (filePath: string, matcher: string | RegExp, replacer: string) => Promise<void>;
}

export function createFsTools(base: SchemaToolContext, policy: ToolsPolicyManager): FsTools {
  // schema 侧普遍按“相对当前工作区根目录”的心智模型传路径，这里统一补绝对化。
  const resolveLocalPath = (targetPath: string) =>
    path.isAbsolute(targetPath) ? targetPath : path.resolve(base.cwd, targetPath);

  return {
    exists: async (filePath) => {
      await policy.assertToolAllowed('fs', 'exists');
      const exchange = await hostCall<{ exists: boolean }>('host.fs.exists', {
        path: filePath,
        cwd: base.cwd,
      });
      return Boolean(exchange.result.exists);
    },
    read: async (filePath) => {
      await policy.assertToolAllowed('fs', 'read');
      return hostFsReadText(base, filePath);
    },
    readJson: async <T>(filePath: string) => JSON.parse(await hostFsReadText(base, filePath)) as T,
    append: async (filePath, content, options) => {
      await policy.assertFsWriteAllowed('append', filePath);
      await hostCall('host.fs.write', {
        path: filePath,
        content,
        append: true,
        mkdirp: options?.mkdirp ?? true,
        cwd: base.cwd,
      });
    },
    write: async (filePath, content, options) => {
      await policy.assertFsWriteAllowed('write', filePath);
      await hostCall('host.fs.write', {
        path: filePath,
        content,
        append: options?.append ?? false,
        mkdirp: options?.mkdirp ?? true,
        cwd: base.cwd,
      });
    },
    writeJson: async (filePath, value, options) => {
      await policy.assertFsWriteAllowed('writeJson', filePath);
      const space = typeof options?.space === 'number' ? options.space : 2;
      await hostCall('host.fs.write', {
        path: filePath,
        content: JSON.stringify(value, null, space),
        append: false,
        mkdirp: options?.mkdirp ?? true,
        cwd: base.cwd,
      });
    },
    mkdir: async (dirPath, options) => {
      await policy.assertFsWriteAllowed('mkdir', dirPath);
      if (options?.recursive ?? false) {
        await hostCall('host.fs.mkdirp', { path: dirPath, cwd: base.cwd });
        return;
      }
      await mkdirFs(resolveLocalPath(dirPath));
    },
    mkdirp: async (dirPath) => {
      await policy.assertFsWriteAllowed('mkdirp', dirPath);
      await hostCall('host.fs.mkdirp', { path: dirPath, cwd: base.cwd });
    },
    remove: async (filePath, options) => {
      await policy.assertFsWriteAllowed('remove', filePath);
      const exchange = await hostCall<{ removed: boolean }>('host.fs.remove', {
        path: filePath,
        recursive: options?.recursive ?? false,
        cwd: base.cwd,
      });
      return { removed: Boolean(exchange.result.removed) };
    },
    copy: async (from, to, options) => {
      await policy.assertFsWriteAllowed('copy', to);
      const fromResolved = resolveLocalPath(from);
      const toResolved = resolveLocalPath(to);
      if (!(options?.overwrite ?? true) && (await fileExists(toResolved))) {
        throw new Error(`copy target already exists: ${to}`);
      }
      await mkdirFs(path.dirname(toResolved), { recursive: true });
      await copyFile(fromResolved, toResolved);
    },
    move: async (from, to, options) => {
      await policy.assertFsWriteAllowed('move', to);
      const fromResolved = resolveLocalPath(from);
      const toResolved = resolveLocalPath(to);
      if (!(options?.overwrite ?? true) && (await fileExists(toResolved))) {
        throw new Error(`move target already exists: ${to}`);
      }
      await mkdirFs(path.dirname(toResolved), { recursive: true });
      await renameFs(fromResolved, toResolved);
    },
    readdir: async (dirPath) => {
      await policy.assertToolAllowed('fs', 'readdir');
      const exchange = await hostCall<{ entries: string[] }>('host.fs.readdir', {
        path: dirPath,
        cwd: base.cwd,
      });
      return Array.isArray(exchange.result.entries) ? exchange.result.entries : [];
    },
    stat: async (filePath) => {
      await policy.assertToolAllowed('fs', 'stat');
      const exchange = await hostCall<{
        isFile: boolean;
        isDir: boolean;
        size: number;
        mtimeMs?: number;
      }>('host.fs.stat', { path: filePath, cwd: base.cwd });
      return exchange.result;
    },
    glob: async (pattern, options) => {
      await policy.assertToolAllowed('fs', 'glob');
      // `glob()` 当前完全在本地递归实现，不依赖 host。
      // 这让它能复用 Node 侧路径处理，同时避免引入额外 bridge 往返。
      const root = resolveLocalPath(options?.cwd ?? '.');
      const matches = await walkLocalFiles(root, options?.onlyFiles ?? true);
      const matcher = globToMatcher(pattern);
      return matches
        .map((entry) => (options?.absolute ? entry.absolute : entry.relative))
        .filter((entry) => matcher(entry));
    },
    ensureFile: async (filePath, content = '') => {
      await policy.assertFsWriteAllowed('ensureFile', filePath);
      const exists = await fileExists(resolveLocalPath(filePath));
      if (!exists) {
        await hostCall('host.fs.write', {
          path: filePath,
          content,
          append: false,
          mkdirp: true,
          cwd: base.cwd,
        });
      }
    },
    ensureDir: async (dirPath) => {
      await policy.assertFsWriteAllowed('ensureDir', dirPath);
      await hostCall('host.fs.mkdirp', { path: dirPath, cwd: base.cwd });
    },
    replace: async (filePath, matcher, replacer) => {
      await policy.assertFsWriteAllowed('replace', filePath);
      const current = await hostFsReadText(base, filePath);
      const next =
        typeof matcher === 'string'
          ? current.split(matcher).join(replacer)
          : current.replace(matcher, replacer);
      await hostCall('host.fs.write', {
        path: filePath,
        content: next,
        append: false,
        mkdirp: true,
        cwd: base.cwd,
      });
    },
  };
}

async function hostFsReadText(base: SchemaToolContext, filePath: string): Promise<string> {
  // 统一从 host 读取文本内容，避免多个 API 重复写同样的 rpc 封装。
  const exchange = await hostCall<{ content: string }>('host.fs.read', {
    path: filePath,
    cwd: base.cwd,
  });
  return String(exchange.result.content ?? '');
}

function globToMatcher(pattern: string): (value: string) => boolean {
  // 这里只实现一套够用的 glob 子集（`*` / `**`），
  // 目标是服务 schema 侧常见文件匹配，而不是完整替代专业 glob 库。
  const normalizedPattern = pattern.replaceAll('\\', '/');
  const escaped = normalizedPattern
    .replaceAll('**', '__DOUBLE_STAR__')
    .replaceAll('*', '__SINGLE_STAR__')
    .replace(/[.+^${}()|[\]\\]/g, '\\$&');
  const regex = escaped
    .replaceAll('/__DOUBLE_STAR__/', '/(?:.*/)?')
    .replaceAll('__DOUBLE_STAR__/', '(?:.*/)?')
    .replaceAll('/__DOUBLE_STAR__', '/.*')
    .replaceAll('__DOUBLE_STAR__', '.*')
    .replaceAll('__SINGLE_STAR__', '[^/]*');
  return (value: string) => new RegExp(`^${regex}$`).test(value.replaceAll('\\', '/'));
}

async function walkLocalFiles(
  root: string,
  onlyFiles: boolean,
): Promise<Array<{ absolute: string; relative: string }>> {
  // 递归收集本地文件树，并同时保留 absolute/relative 两种视图，
  // 方便 `glob()` 按调用方要求返回不同路径形态。
  const out: Array<{ absolute: string; relative: string }> = [];
  const visit = async (dir: string) => {
    const entries = await readdirFs(dir, { withFileTypes: true });
    for (const entry of entries) {
      const absolute = path.join(dir, entry.name);
      const relative = path.relative(root, absolute).replaceAll('\\', '/');
      if (entry.isDirectory()) {
        if (!onlyFiles) {
          out.push({ absolute, relative });
        }
        await visit(absolute);
      } else {
        out.push({ absolute, relative });
      }
    }
  };
  if (await fileExists(root)) {
    const stats = await statFs(root);
    if (stats.isDirectory()) {
      await visit(root);
    } else {
      out.push({ absolute: root, relative: path.basename(root) });
    }
  }
  return out;
}
