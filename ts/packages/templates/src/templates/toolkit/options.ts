/**
 * toolkit 模板的可选项默认值与选项元信息。
 *
 * 主要导出：normalizeToolkitOptions、LINT_TOOL_CHOICES、UNIT_TEST_TOOL_CHOICES、PACKAGE_MANAGER_CHOICES、ToolkitTemplateOptions。
 *
 * toolkit 模板定位：
 * - 用于生成“工具库/SDK”类项目骨架（而不是 SPA 应用）。
 * - buildTool 固定为 vite，releaseStrategy 固定为 `lan-release-native`（由上层流程消费）。
 *
 * 兼容性说明：
 * - `useTs` 兼容 `useTypescript` 与 `language` 字段，以便从不同调用方传参时保持一致。
 */
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
    lintTools: normalizeStringArray(options.lintTools, [
      'eslint',
      'prettier',
      'commitlint',
      'editorconfig',
    ]),
    unitTestTool: normalizeString(options.unitTestTool) ?? 'vitest',
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

export const LINT_TOOL_CHOICES = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'commitlint',
  'editorconfig',
];
export const UNIT_TEST_TOOL_CHOICES = ['vitest'];
export const PACKAGE_MANAGER_CHOICES = ['pnpm', 'npm', 'yarn', 'bun'];

function normalizeString(value: unknown) {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function normalizeStringArray(value: unknown, defaults: string[] = []) {
  if (!Array.isArray(value)) {
    return defaults;
  }
  return value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0);
}
