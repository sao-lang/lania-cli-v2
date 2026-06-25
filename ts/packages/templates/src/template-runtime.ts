/**
 * 模板运行时核心，负责发现模板、读取配置并渲染文件输出。
 *
 * 主要导出：listRuntimeTemplates、getRuntimeTemplate、loadTemplateQuestions、loadTemplateDependencies、loadTemplateOutputTasks、renderRuntimeTemplate。
 * 关键点：
 * - 包含文件系统读写/路径解析
 * - 包含 JSON 协议/序列化
 *
 * 总览：
 * - 模板以目录为单位，入口为 `template.json`（manifest）。
 * - 每个模板可选提供 `questions.*` / `dependencies.*` / `config.*` 等运行时代码，
 *   运行时会优先加载 JS 产物，找不到时再回退到 TS 源文件。
 *
 * 覆盖与扩展：
 * - builtin 模板目录来自 `builtinTemplatesDir()`。
 * - 项目可以通过 `lan.config.*` 的 `templateRuntimeDirs` / `templateDirs` 追加模板根目录。
 * - 当前 discover 行为会把所有 root 的模板都收集起来并按 name 排序；若存在同名模板会同时出现，
 *   由调用方（通常是 Rust 侧）决定优先级/覆盖策略。
 */
import { readdir, readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { asRecord, builtinTemplatesDir, fileExists, loadLanConfig } from './runtime.js';
import { ejsRenderer } from './ejs-renderer.js';

// 模板运行时（Node 侧）：
// - 发现可用模板（builtin + 项目扩展目录）
// - 读取模板 manifest / questions / dependencies
// - 构建 locals 并用 EJS 渲染文件
//
// 约定：
// - 每个模板目录包含 `template.json` 作为入口。
// - 可选提供 `config.*` 自定义 files/outputTasks；否则回退到扫描 `files/**/*.ejs`。
// - locals 同时融合 context 与 options，但会补齐常用默认值（如 packageManager/buildTool/useTs）。

export interface TemplateManifest {
  name: string;
  schemaVersion: number;
  renderEngine: 'rust_declarative' | 'node_bridge';
  legacyTemplateDir: string;
  ownership: 'rust_preferred_node_fallback' | 'node_only' | 'third_party_extension';
  useCases: Array<'create' | 'add'>;
  migrationLayer?: string;
}

interface TemplateRuntimeInput {
  cwd?: string | null;
  context?: Record<string, unknown>;
  options?: Record<string, unknown>;
}

interface TemplateFileRule {
  outputPath: string;
  templatePath?: string;
}

interface TemplateDescriptor {
  manifest: TemplateManifest;
  rootDir: string;
}

export async function listRuntimeTemplates(input: TemplateRuntimeInput = {}) {
  return await discoverTemplates(input);
}

export async function getRuntimeTemplate(
  name: string,
  input: TemplateRuntimeInput = {},
) {
  const templates = await discoverTemplates(input);
  return templates.find((template) => template.manifest.name === name) ?? null;
}

export async function loadTemplateQuestions(
  name: string,
  input: TemplateRuntimeInput = {},
) {
  const template = await requireTemplate(name, input);
  const evaluated = await evaluateTemplateModule(template.rootDir, 'questions', input);
  return normalizeQuestions(evaluated);
}

export async function loadTemplateDependencies(
  name: string,
  input: TemplateRuntimeInput = {},
) {
  const template = await requireTemplate(name, input);
  const evaluated = await evaluateTemplateModule(template.rootDir, 'dependencies', input);
  const record = asRecord(evaluated);
  return {
    dependencies: normalizePackageList(record.dependencies),
    devDependencies: normalizePackageList(record.devDependencies),
  };
}

export async function loadTemplateOutputTasks(
  name: string,
  input: TemplateRuntimeInput = {},
) {
  const template = await requireTemplate(name, input);
  const config = await loadTemplateConfig(template, input);
  return config.outputTasks;
}

export async function renderRuntimeTemplate(
  name: string,
  input: TemplateRuntimeInput = {},
) {
  const template = await requireTemplate(name, input);
  const config = await loadTemplateConfig(template, input);
  const dependencyPayload = await loadTemplateDependencies(name, input);
  const locals = buildTemplateLocals(template, input, dependencyPayload);
  const renderedFiles = [];

  for (const fileRule of config.files) {
    // 约定：未显式指定 templatePath 时，默认映射到 `files/<outputPath>.ejs`。
    const templateFilePath = resolve(
      template.rootDir,
      fileRule.templatePath ?? join('files', `${fileRule.outputPath}.ejs`),
    );
    const content = await ejsRenderer.renderFile(templateFilePath, locals);
    renderedFiles.push({
      path: fileRule.outputPath,
      content,
    });
  }

  return {
    template: template.manifest.name,
    schemaVersion: template.manifest.schemaVersion,
    renderEngine: template.manifest.renderEngine,
    files: renderedFiles,
  };
}

async function discoverTemplates(input: TemplateRuntimeInput) {
  // roots 的顺序会影响模板覆盖：靠后的 root 如果存在同名模板，
  // 目前会与前者同时出现（按 name 排序），调用方需要自行决定优先级。
  const roots = await resolveTemplateRoots(input.cwd ?? null);
  const templates: TemplateDescriptor[] = [];

  for (const rootDir of roots) {
    if (!(await fileExists(rootDir))) {
      continue;
    }
    const entries = await readdir(rootDir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isDirectory()) {
        continue;
      }
      const templateRoot = join(rootDir, entry.name);
      const manifestPath = join(templateRoot, 'template.json');
      if (!(await fileExists(manifestPath))) {
        continue;
      }
      const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as TemplateManifest;
      templates.push({
        manifest,
        rootDir: templateRoot,
      });
    }
  }

  return templates.sort((left, right) =>
    left.manifest.name.localeCompare(right.manifest.name),
  );
}

async function resolveTemplateRoots(cwd: string | null) {
  const roots = [builtinTemplatesDir()];
  if (!cwd) {
    return roots;
  }

  const lanConfig = await loadLanConfig(cwd);
  const config = asRecord(lanConfig.config);
  const product = asRecord(config.product);
  const candidateLists = [
    typeof product.templatesDir === 'string' && product.templatesDir.length > 0
      ? [product.templatesDir]
      : [],
    config.templateRuntimeDirs,
    config.templateDirs,
  ];
  // 兼容两类配置名：templateRuntimeDirs（新）与 templateDirs（旧）。
  for (const candidate of candidateLists) {
    if (!Array.isArray(candidate)) {
      continue;
    }
    for (const value of candidate) {
      if (typeof value === 'string' && value.length > 0) {
        roots.push(resolve(cwd, value));
      }
    }
  }
  return [...new Set(roots)];
}

async function requireTemplate(
  name: string,
  input: TemplateRuntimeInput,
) {
  const template = await getRuntimeTemplate(name, input);
  if (!template) {
    throw new Error(`Unknown template: ${name}`);
  }
  return template;
}

async function loadTemplateConfig(
  template: TemplateDescriptor,
  input: TemplateRuntimeInput,
) {
  // config.* 是可执行模块（可能导出函数），用于动态定制要输出的 files/outputTasks。
  // - 若不存在或返回空 files，则回退为扫描 `files/**/*.ejs` 的“文件即模板”模式。
  const evaluated = (await evaluateTemplateModule(template.rootDir, 'config', input)) ?? {};
  const record = asRecord(evaluated);
  const files = normalizeTemplateFiles(record.files);
  return {
    outputTasks: normalizeStringList(record.outputTasks),
    files:
      files.length > 0
        ? files
        : await scanFileBasedTemplateDirectory(template.rootDir),
  };
}

async function evaluateTemplateModule(
  templateRoot: string,
  moduleName: 'questions' | 'dependencies' | 'config',
  input: TemplateRuntimeInput,
) {
  const modulePath = await resolveTemplateModulePath(templateRoot, moduleName);
  if (!modulePath) {
    return null;
  }
  return await evaluateOptionalModule(modulePath, input);
}

async function resolveTemplateModulePath(
  templateRoot: string,
  moduleName: 'questions' | 'dependencies' | 'config',
) {
  for (const extension of ['.js', '.mjs', '.cjs', '.ts', '.mts', '.cts']) {
    const candidate = join(templateRoot, `${moduleName}${extension}`);
    if (await fileExists(candidate)) {
      return candidate;
    }
  }
  return null;
}

async function evaluateOptionalModule(
  modulePath: string,
  input: TemplateRuntimeInput,
) {
  if (!(await fileExists(modulePath))) {
    return null;
  }
  // 注意：这里会执行模板目录下的本地代码（questions/dependencies/config）。
  // 该行为仅针对本地模板资产，不应用于不可信的远程输入。
  const module = (await import(pathToFileURL(modulePath).href)) as {
    default?: unknown;
  };
  const candidate = module.default ?? module;
  if (typeof candidate === 'function') {
    return await candidate({
      cwd: input.cwd ?? null,
      context: input.context ?? {},
      options: input.options ?? {},
    });
  }
  return candidate;
}

async function scanFileBasedTemplateDirectory(templateRoot: string) {
  const filesRoot = join(templateRoot, 'files');
  if (!(await fileExists(filesRoot))) {
    return [];
  }
  const templateFiles = await listFilesRecursive(filesRoot);
  return templateFiles
    .filter((filePath) => filePath.endsWith('.ejs'))
    .map((filePath) => ({
      outputPath: filePath.slice(filesRoot.length + 1, -4),
      templatePath: filePath.slice(templateRoot.length + 1),
    }));
}

async function listFilesRecursive(rootDir: string): Promise<string[]> {
  const entries = await readdir(rootDir, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const absolutePath = join(rootDir, entry.name);
      if (entry.isDirectory()) {
        return await listFilesRecursive(absolutePath);
      }
      return [absolutePath];
    }),
  );
  return files.flat();
}

function buildTemplateLocals(
  template: TemplateDescriptor,
  input: TemplateRuntimeInput,
  dependencyPayload: {
    dependencies: string[];
    devDependencies: string[];
  },
) {
  // locals 的优先级：options > context（同名字段以 options 为准）。
  // 这使得 CLI flag 更容易覆盖项目默认配置。
  //
  // 默认值策略：
  // - 尽量兼容历史字段（例如 useTs / useTypescript / language）。
  // - 生成模板时需要的常用字段（projectName/buildTool/packageManager 等）会在这里补齐。
  const context = asRecord(input.context);
  const options = asRecord(input.options);
  const projectNameValue = context.projectName;
  const projectName =
    typeof projectNameValue === 'string' && projectNameValue.length > 0
      ? projectNameValue
      : 'lania-app';
  const useTs =
    typeof options.useTs === 'boolean'
      ? options.useTs
      : options.useTypescript !== false &&
        String(options.language ?? 'typescript').toLowerCase() !== 'javascript';
  // `useTs` 支持多个历史字段：useTs / useTypescript / language。
  const lintTools = normalizeStringList(options.lintTools);
  const cssTools = normalizeStringList(options.cssTools);
  const buildTool = typeof options.buildTool === 'string' ? options.buildTool : 'vite';
  const cssProcessor =
    typeof options.cssProcessor === 'string' ? options.cssProcessor : 'css';
  const contextPackageManager =
    typeof context.packageManager === 'string' ? context.packageManager : null;
  const packageManager =
    typeof options.packageManager === 'string'
      ? options.packageManager
      : contextPackageManager ?? 'npm';
  const repository =
    typeof options.repository === 'string'
      ? options.repository
      : typeof context.repository === 'string'
        ? context.repository
        : '';
  const unitTestTool =
    typeof options.unitTestTool === 'string' ? options.unitTestTool : 'vitest';
  const port =
    typeof options.port === 'number' && Number.isFinite(options.port) ? options.port : 3000;
  const resolvedDependencies = normalizeDependencyRecord(options.resolvedDependencies);
  const resolvedDevDependencies = normalizeDependencyRecord(options.resolvedDevDependencies);

  return {
    ...context,
    ...options,
    projectName,
    name: projectName,
    templateName: template.manifest.name,
    options,
    useTs,
    language: typeof options.language === 'string' ? options.language : useTs ? 'TypeScript' : 'JavaScript',
    lintTools,
    cssTools,
    buildTool,
    cssProcessor,
    packageManager,
    repository,
    unitTestTool,
    port,
    dependencies:
      Object.keys(resolvedDependencies).length > 0
        ? resolvedDependencies
        : toDependencyRecord(dependencyPayload.dependencies),
    devDependencies:
      Object.keys(resolvedDevDependencies).length > 0
        ? resolvedDevDependencies
        : toDependencyRecord(dependencyPayload.devDependencies),
    dependencyList: dependencyPayload.dependencies,
    devDependencyList: dependencyPayload.devDependencies,
  };
}

function normalizeDependencyRecord(value: unknown) {
  const record = asRecord(value);
  return Object.fromEntries(
    Object.entries(record).filter(
      ([key, entry]) => key.length > 0 && typeof entry === 'string' && entry.length > 0,
    ),
  ) as Record<string, string>;
}

function normalizeQuestions(value: unknown) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((question) => {
    const record = asRecord(question);
    return {
      name: String(record.name ?? 'projectName'),
      type: String(record.type ?? 'input'),
      ...(record.message ? { message: String(record.message) } : {}),
      ...(Array.isArray(record.choices)
        ? { choices: record.choices.map((choice) => String(choice)) }
        : {}),
    };
  });
}

function normalizePackageList(value: unknown) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((entry) => {
      if (typeof entry === 'string') {
        return entry;
      }
      const record = asRecord(entry);
      if (typeof record.name !== 'string') {
        return null;
      }
      return typeof record.version === 'string'
        ? `${record.name}@${record.version}`
        : record.name;
    })
    .filter((entry): entry is string => Boolean(entry));
}

function normalizeStringList(value: unknown) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter((entry): entry is string => typeof entry === 'string' && entry.length > 0);
}

function normalizeTemplateFiles(value: unknown): TemplateFileRule[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => {
      if (typeof entry === 'string') {
        return {
          outputPath: entry,
        };
      }
      const record = asRecord(entry);
      if (typeof record.outputPath !== 'string' || record.outputPath.length === 0) {
        return null;
      }
      return {
        outputPath: record.outputPath,
        templatePath:
          typeof record.templatePath === 'string' && record.templatePath.length > 0
            ? record.templatePath
            : undefined,
      };
    })
    .filter((entry): entry is TemplateFileRule => Boolean(entry));
}

// Backward-compat exports kept for any callsites still importing these helpers.
// The canonical builtin root is now owned by the templates package itself.
export { builtinTemplatesDir as builtInTemplateRoot } from './runtime.js';

export function templateDirName(filePath: string) {
  return dirname(filePath);
}

function toDependencyRecord(entries: string[]) {
  return Object.fromEntries(
    entries.map((entry) => {
      const versionSeparator = entry.lastIndexOf('@');
      if (versionSeparator > 0) {
        return [entry.slice(0, versionSeparator), entry.slice(versionSeparator + 1)];
      }
      return [entry, 'latest'];
    }),
  );
}
