/**
 * Node 侧运行时工具，负责加载项目配置、工具配置与模块路径。
 *
 * 主要导出：asRecord、loadLanConfig、loadToolConfig、findFirstExisting、fileExists、loadConfigModule。
 * 关键点：
 * - 包含文件系统读写/路径解析
 * - 包含 JSON 协议/序列化
 */
import { access, readFile } from 'node:fs/promises';
import { constants as fsConstants } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, extname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export interface LoadedConfig {
  configPath: string | null;
  config: Record<string, unknown>;
  exists: boolean;
}

const LAN_CONFIG_SEARCH_PLACES = [
  'lan.config.js',
  'lan.config.cjs',
  'lan.config.mjs',
  'lan.config.json',
  'lan.config.ts',
  'lan.config.mts',
  'lan.config.cts',
];

const TOOL_CONFIG_SEARCH_PLACES: Record<string, string[]> = {
  vite: ['vite.config.ts', 'vite.config.js', 'vite.config.cjs'],
  webpack: ['webpack.config.js', 'webpack.config.cjs'],
  rollup: ['rollup.config.js', 'rollup.config.cjs'],
  gulp: ['gulpfile.ts', 'gulpfile.js', 'gulpfile.cjs', 'gulpfile.mjs'],
  eslint: [
    '.eslintrc.js',
    '.eslintrc.cjs',
    'eslint.config.js',
    'eslint.config.cjs',
  ],
  oxlint: [
    '.oxlintrc.json',
    '.oxlintrc.jsonc',
    'oxlint.config.js',
    'oxlint.config.mjs',
    'oxlint.config.cjs',
    'oxlint.config.ts',
    'oxlint.config.mts',
    'oxlint.config.cts',
  ],
  prettier: [
    '.prettierrc',
    '.prettierrc.json',
    '.prettierrc.js',
    '.prettierrc.cjs',
    '.prettierrc.mjs',
    '.prettierrc.ts',
    '.prettierrc.yaml',
    '.prettierrc.yml',
    'prettier.config.js',
    'prettier.config.cjs',
    'prettier.config.mjs',
    'prettier.config.ts',
  ],
  oxfmt: [
    '.oxfmtrc.json',
    '.oxfmtrc.jsonc',
    '.oxfmtrc.js',
    '.oxfmtrc.mjs',
    '.oxfmtrc.cjs',
    '.oxfmtrc.ts',
    '.oxfmtrc.mts',
    '.oxfmtrc.cts',
  ],
  stylelint: [
    '.stylelintrc',
    '.stylelintrc.json',
    '.stylelintrc.js',
    '.stylelintrc.cjs',
    '.stylelintrc.mjs',
    '.stylelintrc.ts',
    '.stylelintrc.yaml',
    '.stylelintrc.yml',
    'stylelint.config.js',
    'stylelint.config.cjs',
    'stylelint.config.mjs',
    'stylelint.config.ts',
  ],
  textlint: [
    '.textlintrc',
    '.textlintrc.json',
    '.textlintrc.js',
    '.textlintrc.cjs',
    '.textlintrc.mjs',
    '.textlintrc.ts',
    '.textlintrc.yaml',
    '.textlintrc.yml',
    'textlint.config.js',
    'textlint.config.cjs',
    'textlint.config.mjs',
    'textlint.config.ts',
  ],
  markdownlint: [
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
  ],
  commitlint: ['commitlint.config.js', 'commitlint.config.cjs'],
  commitizen: ['cz.config.js', 'cz.config.cjs', '.czrc', '.czrc.cjs', '.cz.json'],
  cz: ['cz.config.js', 'cz.config.cjs', '.czrc', '.czrc.cjs', '.cz.json'],
};

export function asRecord(value: unknown): Record<string, unknown> {
  // 运行时从 JSON / 动态 import / 外部 bridge 返回值里拿到的内容经常是 unknown。
  // 这里统一做一次“对象化”收口，避免每个调用点都重复判空和类型断言。
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

export async function loadLanConfig(
  cwd: string,
  options?: { searchFrom?: string; candidates?: string[] },
): Promise<LoadedConfig> {
  // lan.config.* 是 CLI 运行时最核心的一层项目配置。
  // 这里的策略很直接：按候选顺序找到第一个存在的文件，然后把它解析成对象。
  const searchFrom = options?.searchFrom ?? cwd;
  const candidates = options?.candidates ?? LAN_CONFIG_SEARCH_PLACES;
  // 按指定搜索顺序查找 lan.config.*；找到第一个即停止。
  const configPath = await findFirstExisting(searchFrom, candidates);
  if (!configPath) {
    return { configPath: null, config: {}, exists: false };
  }

  return {
    configPath,
    config: await loadConfigModule(configPath),
    exists: true,
  };
}

export async function loadToolConfig(
  cwd: string,
  tool: string,
): Promise<LoadedConfig> {
  // 工具配置来源优先级：
  // 1) 项目文件（如 vite.config.* / .eslintrc.*）
  // 2) package.json 指向的配置文件（部分工具支持）
  // 3) package.json 内联配置（commitizen/commitlint 等）
  const packageJson = await loadPackageJson(cwd);
  // 路径解析优先级是“显式文件优先”：
  // - 用户若已存在工具配置文件，优先使用该文件
  // - 其次才读取 package.json 中的引用/内联字段
  const configPath =
    (await findFirstExisting(cwd, TOOL_CONFIG_SEARCH_PLACES[tool] ?? [])) ??
    (await resolvePackageBackedToolConfigPath(cwd, tool, packageJson));
  if (!configPath) {
    const inlinePackageConfig = inlineToolConfigFromPackageJson(packageJson, tool);
    if (inlinePackageConfig) {
      return {
        configPath: inlinePackageConfig.configPath,
        config: inlinePackageConfig.config,
        exists: true,
      };
    }
    return { configPath: null, config: {}, exists: false };
  }

  return {
    configPath,
    config: await loadConfigFile(configPath),
    exists: true,
  };
}

export async function findFirstExisting(
  cwd: string,
  candidates: string[],
): Promise<string | null> {
  // 有意按传入顺序逐个检查，而不是并发探测。
  // 这样可以让“候选列表顺序”直接表达优先级，行为也更容易解释。
  for (const candidate of candidates) {
    const absolutePath = resolve(cwd, candidate);
    if (await fileExists(absolutePath)) {
      return absolutePath;
    }
  }
  return null;
}

export async function fileExists(path: string): Promise<boolean> {
  // 这里只关心“能不能访问到”，不区分文件/目录，也不把底层错误细节泄漏给上层调用。
  try {
    await access(path, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

export async function loadConfigModule(filePath: string): Promise<Record<string, unknown>> {
  // 历史上调用方区分“模块配置”和“普通配置文件”，但当前实现统一都走 `loadConfigFile`。
  // 保留这个入口主要是为了让上层 API 语义更稳定。
  return loadConfigFile(filePath);
}

export async function loadPackageJsonSnapshot(cwd: string): Promise<Record<string, unknown>> {
  // 提供只读快照接口，避免上层自己拼 `package.json` 路径和解析逻辑。
  return loadPackageJson(cwd);
}

async function loadConfigFile(filePath: string): Promise<Record<string, unknown>> {
  const extension = extname(filePath).toLowerCase();
  if (
    extension === '.json' ||
    extension === '.jsonc' ||
    extension === '.yaml' ||
    extension === '.yml' ||
    extension === '' ||
    filePath.endsWith('.rc')
  ) {
    // 结构化文本：优先按 JSON 解析，失败再退回到 YAML。
    return loadStructuredConfig(filePath);
  }
  // 可执行模块：支持 default export 或 module namespace；
  // 也支持导出函数（返回对象）以便动态生成配置。
  // 注意：这里会执行用户代码，因此该逻辑只应用在项目本地配置，不用于不可信远程输入。
  const moduleNamespace = (await import(pathToFileURL(filePath).href)) as {
    default?: unknown;
  };
  const candidate = moduleNamespace.default ?? moduleNamespace;
  const resolved = typeof candidate === 'function' ? await candidate({}) : candidate;
  return asRecord(resolved);
}

async function loadStructuredConfig(filePath: string): Promise<Record<string, unknown>> {
  // 这里先尝试 JSON，再回退 YAML，覆盖大部分 rc/config 文件的常见格式。
  const content = await readFile(filePath, 'utf8');
  try {
    return asRecord(JSON.parse(content));
  } catch {
    try {
      const { parse } = await import('yaml');
      return asRecord(parse(content));
    } catch {
      // 保守降级：解析失败时返回空对象而不是抛错，
      // 让上层继续执行并通过“exists + validation”给出更友好的提示。
      return {};
    }
  }
}

async function loadPackageJson(cwd: string): Promise<Record<string, unknown>> {
  // package.json 被多个运行时入口复用，这里统一做“存在检查 + 容错解析”。
  const packageJsonPath = resolve(cwd, 'package.json');
  if (!(await fileExists(packageJsonPath))) {
    return {};
  }
  try {
    return asRecord(JSON.parse(await readFile(packageJsonPath, 'utf8')));
  } catch {
    return {};
  }
}

async function resolvePackageBackedToolConfigPath(
  cwd: string,
  tool: string,
  packageJson: Record<string, unknown>,
): Promise<string | null> {
  // 少数工具会把“真正的配置文件路径”写在 package.json 里。
  // 当前只处理 commitizen/cz 这类已知约定，避免把 package.json 解析扩散成无边界规则。
  if (tool !== 'commitizen' && tool !== 'cz') {
    return null;
  }
  const packageConfig = asRecord(packageJson.config);
  const commitizen = asRecord(packageConfig.commitizen);
  const czCustomizable = asRecord(packageConfig['cz-customizable']);
  const configPath =
    typeof commitizen.config === 'string'
      ? commitizen.config
      : typeof czCustomizable.config === 'string'
        ? czCustomizable.config
        : null;

  if (!configPath) {
    return null;
  }
  // package.json 中的相对路径一律按 cwd 解析，避免受当前进程工作目录影响。
  const resolvedPath = resolve(cwd, configPath);
  return (await fileExists(resolvedPath)) ? resolvedPath : null;
}

function inlineToolConfigFromPackageJson(
  packageJson: Record<string, unknown>,
  tool: string,
): { configPath: string; config: Record<string, unknown> } | null {
  // 另一类工具会直接把完整配置内联在 package.json 中。
  // 返回值里的 `configPath` 是逻辑路径，用来告诉上层“配置来自 package.json 的哪个字段”。
  const packageConfig = asRecord(packageJson.config);
  if (tool === 'commitlint') {
    const commitlintConfig =
      objectWithKeys(asRecord(packageJson.commitlint)) ??
      objectWithKeys(asRecord(packageConfig.commitlint));
    if (commitlintConfig) {
      return {
        configPath: 'package.json#commitlint',
        config: commitlintConfig,
      };
    }
  }
  if (tool === 'commitizen' || tool === 'cz') {
    const commitizenConfig =
      objectWithKeys(asRecord(packageJson.commitizen)) ??
      objectWithKeys(asRecord(packageConfig.commitizen));
    if (commitizenConfig) {
      return {
        configPath: 'package.json#commitizen',
        config: commitizenConfig,
      };
    }
  }
  return null;
}

function objectWithKeys(
  value: Record<string, unknown>,
): Record<string, unknown> | null {
  // 空对象通常代表“字段存在但无有效配置”，这里统一把它视为缺失，减少上层判空分支。
  return Object.keys(value).length > 0 ? value : null;
}

export async function resolveModuleFromCwd<T = unknown>(
  cwd: string,
  specifier: string,
): Promise<T | null> {
  // 这个入口允许 runtime 从项目自身依赖树里解析模块，而不是绑死到 node-bridge 包的依赖环境。
  // 对插件/工具桥接场景很关键，因为真正想加载的是“用户项目安装的那份模块”。
  try {
    if (specifier.startsWith('/') || specifier.startsWith('./') || specifier.startsWith('../')) {
      return (await import(pathToFileURL(resolve(cwd, specifier)).href)) as T;
    }
    // 非路径 specifier 通过 createRequire 从 cwd 解析：
    // 可复用项目自身依赖树，而不是 bridge 包自己的 node_modules。
    const requireFromCwd = createRequire(join(cwd, '__lania_bridge__.cjs'));
    return requireFromCwd(specifier) as T;
  } catch {
    return null;
  }
}

export function bridgePackageDir(): string {
  // 返回 node-bridge 包自身目录，供 staging / wrapper / 资源定位等场景复用。
  return dirname(import.meta.dirname);
}

export function normalizeError(error: unknown): { message: string; stack?: string } {
  // 对外统一暴露稳定的错误结构，避免把各种非 Error throw 值直接泄漏到日志协议里。
  if (error instanceof Error) {
    return { message: error.message, stack: error.stack };
  }
  return { message: String(error) };
}
