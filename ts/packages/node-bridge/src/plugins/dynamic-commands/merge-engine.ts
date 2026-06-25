/**
 * 负责把 scaffold 产物应用到现有工作区里，并在允许的策略下尽量保留用户已有内容。
 *
 * merge engine 的输入已经是“渲染完成的模板文件 + 依赖补丁计划”，它不负责决定该生成
 * 什么，只负责决定生成结果应当如何与磁盘上的现有文件共存。
 */
import type { ScaffoldDependencyPlanResult, ScaffoldTemplateFile } from '../../core/schema-tools.js';
import type { ScaffoldMergeRulePlan } from './types.js';

export interface MergeEngineFileResult {
  path: string;
  content: string;
  strategy: string;
  source: 'rendered' | 'merged';
  existed: boolean;
  change: 'create' | 'replace' | 'merge';
}

export interface MergeEngineResult {
  files: MergeEngineFileResult[];
}

export interface MergeEngineInput {
  renderedFiles: ScaffoldTemplateFile[];
  dependencyPlan: ScaffoldDependencyPlanResult;
  mergeRules: ScaffoldMergeRulePlan[];
  readExistingFile: (filePath: string) => Promise<string | null>;
  readExistingPackageJson?: () => Promise<Record<string, unknown> | null>;
}

interface MergeTargetInput {
  path: string;
  renderedContent: string | undefined;
  existingContent: string | null;
  mergeRule: ScaffoldMergeRulePlan | undefined;
  dependencyPlan: ScaffoldDependencyPlanResult;
  readExistingPackageJson?: () => Promise<Record<string, unknown> | null>;
}

type ArrayMergeStrategy = 'append' | 'dedupe';

/**
 * 计算一次 scaffold 执行最终要落盘的文件集合。
 *
 * 每个目标路径都会结合“模板渲染结果、磁盘现状、可选 merge rule”评估一次。
 * `package.json` 会被特殊处理，因为依赖 recipe 可能需要在模板没有生成它的情况下，
 * 仍然去创建或补丁这个文件。
 */
export async function mergeScaffoldFiles(input: MergeEngineInput): Promise<MergeEngineResult> {
  const renderedFilesByPath = new Map(input.renderedFiles.map((file) => [file.path, file.content]));
  const mergeRulesByTarget = new Map(input.mergeRules.map((rule) => [rule.target, rule]));
  const targetPaths = new Set(renderedFilesByPath.keys());

  if (shouldMaterializePackageJson(renderedFilesByPath.get('package.json'), input.dependencyPlan)) {
    targetPaths.add('package.json');
  }

  const files: MergeEngineFileResult[] = [];
  for (const path of [...targetPaths].sort((left, right) => left.localeCompare(right))) {
    const renderedContent = renderedFilesByPath.get(path);
    const existingContent = await input.readExistingFile(path);
    const result = await mergeTargetFile({
      path,
      renderedContent,
      existingContent,
      mergeRule: mergeRulesByTarget.get(path),
      dependencyPlan: input.dependencyPlan,
      readExistingPackageJson: input.readExistingPackageJson,
    });
    if (result) {
      files.push(result);
    }
  }

  return { files };
}

/**
 * 计算单个目标文件的最终内容。
 *
 * 各种策略的语义如下：
 * - `replace`：直接用生成内容替换现有内容
 * - `append` / `patch`：按文本追加到原文件末尾
 * - `deep_merge`：按 JSON 对象递归合并，数组直接拼接
 * - `dedupe_merge`：同样递归合并，但数组会按序列化值去重
 */
async function mergeTargetFile(input: MergeTargetInput): Promise<MergeEngineFileResult | null> {
  if (input.path === 'package.json') {
    const content = await mergePackageJsonContent(input);
    if (content === null) {
      return null;
    }
    return {
      path: input.path,
      content,
      strategy: input.mergeRule?.strategy ?? 'package_json',
      source: input.existingContent === null ? 'rendered' : 'merged',
      existed: input.existingContent !== null,
      change: input.existingContent === null ? 'create' : 'merge',
    };
  }

  if (typeof input.renderedContent !== 'string') {
    return null;
  }

  if (input.existingContent === null) {
    return {
      path: input.path,
      content: input.renderedContent,
      strategy: input.mergeRule?.strategy ?? 'replace',
      source: 'rendered',
      existed: false,
      change: 'create',
    };
  }

  const strategy = input.mergeRule?.strategy ?? 'replace';
  switch (strategy) {
    case 'replace':
      return {
        path: input.path,
        content: input.renderedContent,
        strategy,
        source: 'rendered',
        existed: true,
        change: 'replace',
      };
    case 'append':
    case 'patch':
      return {
        path: input.path,
        content: appendText(input.existingContent, input.renderedContent),
        strategy,
        source: 'merged',
        existed: true,
        change: 'merge',
      };
    case 'deep_merge':
      return {
        path: input.path,
        content: mergeStructuredFile(input.existingContent, input.renderedContent, 'append'),
        strategy,
        source: 'merged',
        existed: true,
        change: 'merge',
      };
    case 'dedupe_merge':
      return {
        path: input.path,
        content: mergeStructuredFile(input.existingContent, input.renderedContent, 'dedupe'),
        strategy,
        source: 'merged',
        existed: true,
        change: 'merge',
      };
    default:
      return {
        path: input.path,
        content: input.renderedContent,
        strategy: 'replace',
        source: 'rendered',
        existed: true,
        change: 'replace',
      };
  }
}

/**
 * 合并 scaffold 视角下最终生效的 `package.json`。
 *
 * 如果模板内容是合法 JSON，这里会优先走结构化合并；如果模板本身不是合法 JSON，
 * 则故意退回到原始文本，避免把自定义模板直接判死。这样既能支持规范模板的依赖补丁，
 * 也不会无谓阻断特殊场景。
 */
async function mergePackageJsonContent(input: MergeTargetInput): Promise<string | null> {
  const renderedRecord = parseJsonObject(input.renderedContent);
  const existingRecord =
    parseJsonObject(input.existingContent ?? undefined) ??
    (await input.readExistingPackageJson?.()) ??
    null;

  if (!renderedRecord && !existingRecord && !hasPackageJsonPatch(input.dependencyPlan)) {
    return null;
  }

  if (input.renderedContent && !renderedRecord) {
    return input.renderedContent;
  }

  const merged = mergePackageJsonRecords(existingRecord, renderedRecord, input.dependencyPlan);
  return merged ? `${JSON.stringify(merged, null, 2)}\n` : null;
}

/**
 * 按固定优先级合并 package.json 的多个来源：
 * 现有工作区内容 -> 模板渲染结果 -> dependency recipe 补丁。
 *
 * 把 recipe 补丁放在最后，是为了让 preset / feature 驱动的依赖结论能稳定覆盖用户旧值
 * 和模板默认值，避免 scaffold 计划与最终落盘结果不一致。
 */
export function mergePackageJsonRecords(
  existing: Record<string, unknown> | null,
  rendered: Record<string, unknown> | null,
  dependencyPlan: ScaffoldDependencyPlanResult,
): Record<string, unknown> | null {
  if (!existing && !rendered && !hasPackageJsonPatch(dependencyPlan)) {
    return null;
  }

  const base = deepMergeRecords(existing ?? {}, rendered ?? {}, { arrayStrategy: 'dedupe' });
  const dependencies = mergeStringRecordSections(
    existing?.dependencies,
    rendered?.dependencies,
    dependencyPlan.packageJsonPatch.dependencies,
  );
  const devDependencies = mergeStringRecordSections(
    existing?.devDependencies,
    rendered?.devDependencies,
    dependencyPlan.packageJsonPatch.devDependencies,
  );
  const scripts = mergeStringRecordSections(
    existing?.scripts,
    rendered?.scripts,
    dependencyPlan.packageJsonPatch.scripts,
  );

  return {
    ...base,
    dependencies,
    devDependencies,
    scripts,
    packageManager: resolvePackageManager(existing, rendered, dependencyPlan),
  };
}

/**
 * 决定 package.json 中最终写入的 package manager 标记。
 *
 * 这里优先保留工作区中已经显式存在的值，因为那通常代表当前仓库的既有约定。只有当
 * 现有文件和模板都没有提供时，才退回到 scaffold 依赖计划里推断出的管理器。
 */
function resolvePackageManager(
  existing: Record<string, unknown> | null,
  rendered: Record<string, unknown> | null,
  dependencyPlan: ScaffoldDependencyPlanResult,
): string {
  const candidate = existing?.packageManager ?? rendered?.packageManager;
  return typeof candidate === 'string' && candidate.length > 0 ? candidate : dependencyPlan.manager;
}

/**
 * 合并只接受字符串值的 record 区块，比如 `dependencies`、`devDependencies`、`scripts`。
 *
 * 后输入覆盖前输入，这样 recipe 只需要声明自己关心的命令或版本，不必把整个区块完整重写。
 */
function mergeStringRecordSections(
  existing: unknown,
  rendered: unknown,
  patch: Record<string, string>,
): Record<string, string> {
  return {
    ...asStringRecord(existing),
    ...asStringRecord(rendered),
    ...patch,
  };
}

/**
 * 递归合并结构化 JSON 内容。
 *
 * 这里唯一交给调用方决定的是数组策略：有些目标文件依赖追加顺序，有些更像集合语义，
 * 需要在合并时去重。
 */
function mergeStructuredFile(
  existingContent: string,
  renderedContent: string,
  arrayStrategy: ArrayMergeStrategy,
): string {
  const existing = parseJsonObject(existingContent);
  const rendered = parseJsonObject(renderedContent);
  if (!existing || !rendered) {
    return renderedContent;
  }
  return `${JSON.stringify(deepMergeRecords(existing, rendered, { arrayStrategy }), null, 2)}\n`;
}

/**
 * 深度合并普通对象，并在写入时克隆 incoming 值，避免不同合并阶段之间共享可变引用。
 */
export function deepMergeRecords(
  base: Record<string, unknown>,
  incoming: Record<string, unknown>,
  options: { arrayStrategy: ArrayMergeStrategy },
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(incoming)) {
    const current = result[key];
    if (Array.isArray(current) && Array.isArray(value)) {
      result[key] =
        options.arrayStrategy === 'dedupe'
          ? uniqueUnknowns([...current, ...value])
          : [...current, ...value];
      continue;
    }
    if (isPlainRecord(current) && isPlainRecord(value)) {
      result[key] = deepMergeRecords(current, value, options);
      continue;
    }
    result[key] = cloneUnknown(value);
  }
  return result;
}

function shouldMaterializePackageJson(
  renderedPackageJson: string | undefined,
  dependencyPlan: ScaffoldDependencyPlanResult,
): boolean {
  return typeof renderedPackageJson === 'string' || hasPackageJsonPatch(dependencyPlan);
}

/**
 * 判断 dependency plan 是否会修改 package.json。
 *
 * 即使模板没有生成 package.json，只要依赖补丁非空，也应该把这个文件纳入输出集合，
 * 这样 recipe 才能给最小模板补上依赖和 scripts。
 */
function hasPackageJsonPatch(dependencyPlan: ScaffoldDependencyPlanResult): boolean {
  return (
    Object.keys(dependencyPlan.packageJsonPatch.dependencies).length > 0 ||
    Object.keys(dependencyPlan.packageJsonPatch.devDependencies).length > 0 ||
    Object.keys(dependencyPlan.packageJsonPatch.scripts).length > 0
  );
}

/**
 * 以尽量稳定的换行边界把两段文本拼起来。
 */
function appendText(existing: string, incoming: string): string {
  if (!existing.trim()) {
    return incoming;
  }
  if (!incoming.trim()) {
    return existing;
  }
  return existing.endsWith('\n') ? `${existing}${incoming}` : `${existing}\n${incoming}`;
}

/**
 * 只在内容是“顶层对象 JSON”时才返回可合并结果。
 *
 * 数组和标量会被直接拒绝，因为当前 merge 逻辑只知道如何处理 record 形状的文档。
 */
function parseJsonObject(content: string | null | undefined): Record<string, unknown> | null {
  if (!content) {
    return null;
  }
  try {
    const parsed = JSON.parse(content);
    return isPlainRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function asStringRecord(value: unknown): Record<string, string> {
  if (!isPlainRecord(value)) {
    return {};
  }
  const result: Record<string, string> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (key.length > 0 && typeof entry === 'string' && entry.length > 0) {
      result[key] = entry;
    }
  }
  return result;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}

/**
 * 用 JSON 序列化结果作为结构化 key，对数组做去重。
 *
 * 这里的实现刻意保持简单：scaffold 阶段的数组通常是原始值或小型 JSON 片段，
 * “保留首次出现顺序”比支持复杂对象图更重要。
 */
function uniqueUnknowns(values: unknown[]): unknown[] {
  const result: unknown[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    const key = JSON.stringify(value);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    result.push(cloneUnknown(value));
  }
  return result;
}

/**
 * 深拷贝普通 JSON 风格的值，让合并结果可以被调用方安全地当作自有结构继续处理。
 */
function cloneUnknown<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map((entry) => cloneUnknown(entry)) as T;
  }
  if (isPlainRecord(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, cloneUnknown(entry)]),
    ) as T;
  }
  return value;
}
