/**
 * `tools.config`：围绕项目配置文件的查找、加载和归一化能力。
 *
 * 这层主要做三件事：
 * - 复用 `core/runtime` 里已经定义好的配置发现规则；
 * - 把 lan/tool/package 这几类常见配置统一成一组稳定 API；
 * - 在进入真正的配置读取前补上策略校验。
 *
 * 它不负责解释配置语义，只负责告诉调用方“配置文件在哪、有没有、内容长什么样”。
 */
import { readFile } from 'node:fs/promises';
import path from 'node:path';

import {
  asRecord,
  bridgePackageDir,
  fileExists,
  findFirstExisting,
  loadConfigModule,
  loadLanConfig,
  loadToolConfig,
  normalizeError,
  resolveModuleFromCwd,
} from '../core/runtime.js';

export interface ConfigTools {
  asRecord: (value: unknown) => Record<string, unknown>;
  loadLan: (
    cwd?: string,
    options?: { searchFrom?: string; candidates?: string[] },
  ) => Promise<unknown>;
  loadTool: (tool: string, cwd?: string) => Promise<unknown>;
  findFirstExisting: (cwd: string, candidates: string[]) => Promise<string | null>;
  fileExists: (filePath: string) => Promise<boolean>;
  loadModule: (filePath: string) => Promise<Record<string, unknown>>;
  resolveModuleFromCwd: <T = unknown>(specifier: string, cwd?: string) => Promise<T | null>;
  bridgePackageDir: () => string;
  normalizeError: (error: unknown) => { message: string; stack?: string };
  supportedTypes: () => string[];
  searchPlaces: (type: string) => string[];
  search: (
    typeOrModule: string,
    from?: string,
  ) => Promise<{ exists: boolean; configPath: string | null }>;
  load: (
    typeOrModule: string,
    configPath?: string | null,
    options?: { cwd?: string },
  ) => Promise<{ exists: boolean; configPath: string | null; config: Record<string, unknown> }>;
}

const SUPPORTED_TOOL_CONFIGS = new Set([
  'vite',
  'webpack',
  'rollup',
  'gulp',
  'eslint',
  'prettier',
  'stylelint',
  'markdownlint',
  'textlint',
  'cz',
  'commitlint',
  'commitizen',
]);

const SUPPORTED_CONFIG_TYPES = [
  'lan',
  'package',
  'vite',
  'webpack',
  'rollup',
  'gulp',
  'eslint',
  'prettier',
  'stylelint',
  'markdownlint',
  'textlint',
  'commitlint',
  'cz',
  'commitizen',
  'editorconfig',
  'tsc',
  'npm',
  'pnpm',
] as const;

export function createConfigTools(
  base: { cwd: string },
  policy: { assertConfigAllowed: (operation: string) => Promise<void> },
): ConfigTools {
  return {
    asRecord,
    loadLan: async (cwd, options) => {
      await policy.assertConfigAllowed('loadLan');
      return loadLanConfig(cwd ?? base.cwd, options);
    },
    loadTool: async (tool, cwd) => {
      await policy.assertConfigAllowed('loadTool');
      return loadToolConfig(cwd ?? base.cwd, tool);
    },
    findFirstExisting,
    fileExists,
    loadModule: async (filePath) => {
      await policy.assertConfigAllowed('loadModule');
      return loadConfigModule(filePath);
    },
    resolveModuleFromCwd: <T>(specifier: string, cwd?: string) =>
      resolveModuleFromCwd<T>(cwd ?? base.cwd, specifier),
    bridgePackageDir,
    normalizeError,
    supportedTypes: () => [...SUPPORTED_CONFIG_TYPES],
    searchPlaces: (type) => searchPlacesForType(type),
    search: async (typeOrModule, from) => {
      await policy.assertConfigAllowed('search');
      const cwd = from ?? base.cwd;
      // `search()` 只负责按配置类型返回“命中的第一个候选路径”，不直接读取内容。
      const places = searchPlacesForType(typeOrModule);
      if (places.length === 0) {
        return { exists: false, configPath: null };
      }
      const configPath = await findFirstExisting(cwd, places);
      return { exists: Boolean(configPath), configPath };
    },
    load: async (typeOrModule, configPath, options) => {
      await policy.assertConfigAllowed('load');
      const cwd = options?.cwd ?? base.cwd;
      // `load()` 同时兼容三类入口：
      // - 预定义类型（lan/package/各类工具配置）
      // - 显式给出的 configPath
      // - 查不到时返回统一的 `{ exists: false, ... }`
      if (typeOrModule === 'lan') {
        const loaded = await loadLanConfig(cwd);
        return { exists: loaded.exists, configPath: loaded.configPath, config: loaded.config };
      }
      if (typeOrModule === 'package') {
        const pkgPath = configPath ?? path.resolve(cwd, 'package.json');
        const exists = await fileExists(pkgPath);
        return {
          exists,
          configPath: exists ? pkgPath : null,
          config: exists ? asRecord(JSON.parse(await readFile(pkgPath, 'utf8'))) : {},
        };
      }

      if (SUPPORTED_TOOL_CONFIGS.has(typeOrModule)) {
        const loaded = await loadToolConfig(cwd, typeOrModule);
        return { exists: loaded.exists, configPath: loaded.configPath, config: loaded.config };
      }

      if (configPath) {
        const resolved = path.isAbsolute(configPath) ? configPath : path.resolve(cwd, configPath);
        const exists = await fileExists(resolved);
        return {
          exists,
          configPath: exists ? resolved : null,
          config: exists ? await loadConfigModule(resolved) : {},
        };
      }

      return { exists: false, configPath: null, config: {} };
    },
  };
}

function searchPlacesForType(type: string): string[] {
  // 这里只维护少量 schema 层高频配置类型。
  // 更复杂的工具配置发现逻辑仍然收敛在 `core/runtime` 中。
  switch (type) {
    case 'editorconfig':
      return ['.editorconfig'];
    case 'tsc':
      return ['tsconfig.json', 'tsconfig.base.json'];
    case 'npm':
      return ['.npmrc'];
    case 'pnpm':
      return ['pnpm-workspace.yaml', 'pnpm-workspace.yml', 'pnpm-lock.yaml'];
    case 'package':
      return ['package.json'];
    case 'lan':
      return [
        'lan.config.js',
        'lan.config.cjs',
        'lan.config.mjs',
        'lan.config.json',
        'lan.config.ts',
        'lan.config.mts',
        'lan.config.cts',
      ];
    case 'gulp':
      return ['gulpfile.ts', 'gulpfile.js', 'gulpfile.cjs', 'gulpfile.mjs'];
    case 'markdownlint':
      return [
        '.markdownlint.json',
        '.markdownlint.jsonc',
        '.markdownlint.yaml',
        '.markdownlint.yml',
        '.markdownlint.js',
        '.markdownlint.cjs',
        '.markdownlint.mjs',
        '.markdownlint.ts',
        'markdownlint.config.js',
        'markdownlint.config.cjs',
        'markdownlint.config.mjs',
        'markdownlint.config.ts',
      ];
    case 'cz':
      return ['cz.config.js', 'cz.config.cjs', '.czrc', '.czrc.cjs', '.cz.json'];
    default:
      return [];
  }
}
