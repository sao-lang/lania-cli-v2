/**
 * templates 包的运行时基础工具，用于定位内置模板目录并加载配置。
 *
 * 主要导出：asRecord、loadLanConfig、findFirstExisting、fileExists、loadConfigModule、templatesPackageDir。
 * 关键点：
 * - 包含文件系统读写/路径解析
 *
 * builtinTemplatesDir 选择逻辑：
 * - workspace 开发态：优先使用 `src/templates`（便于本地调试模板内容）
 * - 发布态：优先使用 `dist/templates`（运行时不依赖 TS 源码）
 *
 * 这里用 `import.meta.dirname.includes('/src')` 作为“开发态”的轻量判断，
 * 同时用 `existsSync(distTemplatesDir)` 兜底处理未构建产物的情况。
 */
import { constants as fsConstants, existsSync } from 'node:fs';
import { access } from 'node:fs/promises';
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export interface LoadedConfig {
  configPath: string | null;
  config: Record<string, unknown>;
  exists: boolean;
}

const LAN_CONFIG_SEARCH_PLACES = ['lan.config.js', 'lan.config.cjs', 'lan.config.ts', 'lan.config.json'];

export function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

export async function loadLanConfig(cwd: string): Promise<LoadedConfig> {
  const configPath = await findFirstExisting(cwd, LAN_CONFIG_SEARCH_PLACES);
  if (!configPath) {
    return { configPath: null, config: {}, exists: false };
  }

  return {
    configPath,
    config: await loadConfigModule(configPath),
    exists: true,
  };
}

export async function findFirstExisting(
  cwd: string,
  candidates: string[],
): Promise<string | null> {
  for (const candidate of candidates) {
    const absolutePath = resolve(cwd, candidate);
    if (await fileExists(absolutePath)) {
      return absolutePath;
    }
  }
  return null;
}

export async function fileExists(path: string): Promise<boolean> {
  try {
    await access(path, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

export async function loadConfigModule(filePath: string): Promise<Record<string, unknown>> {
  if (filePath.endsWith('.json')) {
    const raw = await readFile(filePath, 'utf8');
    return asRecord(JSON.parse(raw));
  }
  const moduleNamespace = (await import(pathToFileURL(filePath).href)) as {
    default?: unknown;
  };
  const candidate = moduleNamespace.default ?? moduleNamespace;
  const resolved = typeof candidate === 'function' ? await candidate({}) : candidate;
  return asRecord(resolved);
}

export function templatesPackageDir(): string {
  return dirname(import.meta.dirname);
}

export function builtinTemplatesDir(): string {
  const sourceTemplatesDir = join(templatesPackageDir(), 'src', 'templates');
  const distTemplatesDir = join(templatesPackageDir(), 'dist', 'templates');
  return import.meta.dirname.includes('/src') || !existsSync(distTemplatesDir)
    ? sourceTemplatesDir
    : distTemplatesDir;
}
