/**
 * toolkit-monorepo 模板的可选项默认值与选项元信息。
 *
 * 主要导出：normalizeToolkitMonorepoOptions、ToolkitMonorepoTemplateOptions。
 *
 * 与单包 toolkit 的差异：
 * - 目标是生成一个基于 pnpm workspace 的 monorepo（默认 releaseStrategy=changesets）。
 * - 这里的 useTs 判定更“简单”：默认开启，只有显式传入 useTs=false 才关闭。
 *   这样可以让模板默认更贴近现代 TS 工具链。
 */
import { choicesFor, templateChoicesFor } from '../../presentation.js';

/** toolkit 模板族（toolkit / toolkit-monorepo）共用的工程化工具选项。 */
const TOOLKIT_TEMPLATE_CHOICES = templateChoicesFor('toolkit');

export interface ToolkitMonorepoTemplateOptions {
  /** 工程化工具集合（eslint/oxlint/prettier/oxfmt/commitlint/editorconfig）。 */
  lintTools: string[];
  /** 单测工具；当前默认是 vitest。 */
  unitTestTool: string | null;
  /** 包管理器；monorepo 场景通常更偏向 pnpm。 */
  packageManager: string | null;
  /** 仓库地址（可为空）。 */
  repository: string;
  /** 是否使用 TypeScript；默认 true，显式传 false 才会关闭。 */
  useTs: boolean;
  /** 是否跳过 git 初始化相关流程。 */
  skipGit: boolean;
  /** 是否跳过依赖安装。 */
  skipInstall: boolean;
  /** 是否显式指定了 skipInstall。 */
  skipInstallSpecified: boolean;
  /** monorepo 模板固定使用 vite。 */
  buildTool: 'vite';
  /** monorepo 默认采用 changesets 作为发布策略。 */
  releaseStrategy: 'changesets';
}

export function normalizeToolkitMonorepoOptions(options: Record<string, unknown> = {}) {
  // monorepo 默认使用 TypeScript，只有显式传入 false 才关闭。
  const useTs = options.useTs !== false;

  return {
    lintTools: normalizeStringArray(
      options.lintTools,
      TOOLKIT_TEMPLATE_CHOICES.defaultLintTools,
    ),
    unitTestTool: normalizeString(options.unitTestTool) ?? choicesFor('node').defaultTestTool,
    packageManager: normalizeString(options.packageManager),
    repository: normalizeString(options.repository) ?? '',
    useTs,
    skipGit: options.skipGit === true,
    skipInstall: options.skipInstall === true,
    skipInstallSpecified: options.skipInstallSpecified === true,
    buildTool: 'vite',
    releaseStrategy: 'changesets',
  };
}

/**
 * 工程化工具 / 包管理器 / 测试运行器来自运行时工具链表（B14）：
 * - `NODE_TEMPLATE_CHOICES.toolkit`（`presentation.ts`）是 lint 与包管理器的唯一来源；
 * - 测试运行器取 node 运行时 presentation 的默认测试工具（`vitest`）。
 *
 * 注意：monorepo 的测试运行器问答只提供 `vitest`（不提供 `skip`），这是既有行为；
 * `choices` 直接用 `UNIT_TEST_TOOL_CHOICES` 保持与改动前一致。
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
