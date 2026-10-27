/**
 * toolkit 模板的可选项默认值与选项元信息。
 *
 * 主要导出：normalizeToolkitOptions、LINT_TOOL_CHOICES、UNIT_TEST_TOOL_CHOICES、PACKAGE_MANAGER_CHOICES、PACKAGE_MANAGER_DEFAULT、ToolkitTemplateOptions。
 *
 * toolkit 模板定位：
 * - 用于生成“工具库/SDK”类项目骨架（而不是 SPA 应用）。
 * - buildTool 固定为 vite，releaseStrategy 固定为 `lan-release-native`（由上层流程消费）。
 *
 * 选项来源（B14）：
 * - 工程化工具 / 包管理器来自运行时工具链表 `presentation.ts`（`NODE_TEMPLATE_CHOICES.toolkit`），
 *   本文件不再维护第二份字面量；
 * - 测试运行器取 node 运行时 presentation 的默认测试工具。
 *
 * 兼容性说明：
 * - `useTs` 兼容 `useTypescript` 与 `language` 字段，以便从不同调用方传参时保持一致。
 */
import { choicesFor, templateChoicesFor } from '../../presentation.js';

/** toolkit 模板族（toolkit / toolkit-monorepo）共用的工程化工具选项。 */
const TOOLKIT_TEMPLATE_CHOICES = templateChoicesFor('toolkit');

export interface ToolkitTemplateOptions {
  /** 选择启用的工程化工具集合（eslint/oxlint/prettier/oxfmt/commitlint/editorconfig）。 */
  lintTools: string[];
  /**
   * 单测工具：
   * - null 表示跳过测试能力（questions.ts 会提供 "skip" 选项）
   * - 当前默认使用 vitest
   */
  unitTestTool: string | null;
  /** 包管理器（null 表示让 questions.ts 询问）。 */
  packageManager: string | null;
  /** 仓库地址（可为空；skipGit=true 时一般不会询问）。 */
  repository: string;
  /** 是否使用 TypeScript（影响依赖与模板文件内容）。 */
  useTs: boolean;
  skipGit: boolean;
  skipInstall: boolean;
  skipInstallSpecified: boolean;
  /** toolkit 模板固定使用 vite（对齐 dts/构建生态）。 */
  buildTool: 'vite';
  /** 发布策略由上层实现，模板只提供一个稳定标识。 */
  releaseStrategy: 'lan-release-native';
}

export function normalizeToolkitOptions(
  options: Record<string, unknown> = {},
): ToolkitTemplateOptions {
  const useTs =
    typeof options.useTs === 'boolean'
      ? options.useTs
      : options.useTypescript !== false &&
        String(options.language ?? 'typescript').toLowerCase() !== 'javascript';

  return {
    lintTools: normalizeStringArray(options.lintTools, TOOLKIT_TEMPLATE_CHOICES.defaultLintTools),
    unitTestTool: normalizeString(options.unitTestTool) ?? choicesFor('node').defaultTestTool,
    packageManager: normalizeString(options.packageManager),
    repository: normalizeString(options.repository) ?? '',
    useTs,
    skipGit: options.skipGit === true,
    skipInstall: options.skipInstall === true,
    skipInstallSpecified: options.skipInstallSpecified === true,
    buildTool: 'vite',
    releaseStrategy: 'lan-release-native',
  };
}

/**
 * 工程化工具 / 包管理器 / 测试运行器来自运行时工具链表（B14）：
 * - `NODE_TEMPLATE_CHOICES.toolkit`（`presentation.ts`）是 lint 与包管理器的唯一来源；
 * - 测试运行器取 node 运行时 presentation 的默认测试工具（`vitest`），
 *   避免又写一份 `['vitest']`。
 */
export const LINT_TOOL_CHOICES = TOOLKIT_TEMPLATE_CHOICES.lintToolValues;
export const UNIT_TEST_TOOL_CHOICES = [choicesFor('node').defaultTestTool];
export const PACKAGE_MANAGER_CHOICES = TOOLKIT_TEMPLATE_CHOICES.packageManagerValues;
/** 包管理器问答默认值；`null` 时 questions 不写 `default`。 */
export const PACKAGE_MANAGER_DEFAULT = TOOLKIT_TEMPLATE_CHOICES.defaultPackageManager;

function normalizeString(value: unknown) {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function normalizeStringArray(value: unknown, defaults: string[] = []) {
  if (!Array.isArray(value)) {
    return defaults;
  }
  return value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0);
}
