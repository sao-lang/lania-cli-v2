/**
 * spa-vue 模板的可选项默认值与选项元信息。
 *
 * 主要导出：normalizeSpaVueOptions、CSS_PROCESSOR_CHOICES、CSS_TOOL_CHOICES、LINT_TOOL_CHOICES、PACKAGE_MANAGER_CHOICES、SpaVueTemplateOptions。
 *
 * 这份 options 的定位：
 * - `questions.ts`：负责“问用户什么”（交互层）
 * - `options.ts`：负责“把各种来源的输入归一化”（数据层），并提供 choices 常量给 questions/config/dependencies 复用
 * - `config.ts` / `dependencies.ts`：基于 normalized options 产出文件规则与依赖清单（产出层）
 *
 * 兼容性说明：
 * - `useTs` 同时兼容 `useTypescript` 与 `language`（历史字段），避免不同调用方传参不一致导致模板抖动。
 */
export interface SpaVueTemplateOptions {
  /**
   * 项目类型（模板可能会用它隐藏/显示一些问题项）。
   * 例如某些 projectType（nodejs/toolkit）不需要 css 相关问题。
   */
  projectType: string;
  /** CSS 预处理器类型（决定生成何种样式文件与依赖）。 */
  cssProcessor: 'css' | 'less' | 'sass' | 'stylus';
  /** CSS 生态工具集合（例如 tailwindcss）。 */
  cssTools: string[];
  /** 工程化工具集合（eslint/oxlint/prettier/oxfmt/stylelint/commitlint/editorconfig）。 */
  lintTools: string[];
  /** 构建工具：vite 或 webpack（决定模板文件与 devDependencies）。 */
  buildTool: 'vite' | 'webpack';
  /**
   * 包管理器。
   * - null 表示未指定，questions.ts 会引导用户选择
   * - 指定后模板会据此生成 lockfile/脚本/提示语（由上层执行）
   */
  packageManager: string | null;
  /** 仓库地址，用于生成 package.json 的 repository 等字段（可为空）。 */
  repository: string;
  /** 语言偏好（最终会决定 useTs）。 */
  language: 'TypeScript' | 'JavaScript';
  /** 是否使用 TypeScript（会影响模板文件、tsconfig、类型依赖等）。 */
  useTs: boolean;
  /** 是否跳过 git 初始化（影响是否询问 repository 等）。 */
  skipGit: boolean;
  /** 是否跳过依赖安装（由上层执行 install-deps 时使用）。 */
  skipInstall: boolean;
  /** 是否显式指定了 skipInstall（用于区分默认值 vs 用户输入）。 */
  skipInstallSpecified: boolean;
  /** dev server 端口（影响 vite/webpack 配置与展示）。 */
  port: number;
}

export function normalizeSpaVueOptions(
  options: Record<string, unknown> = {},
): SpaVueTemplateOptions {
  const useTs =
    typeof options.useTs === 'boolean'
      ? options.useTs
      : options.useTypescript !== false &&
        String(options.language ?? 'typescript').toLowerCase() !== 'javascript';
  // 语言字段主要用于“显式选择 JavaScript”时强制 useTs=false。
  // 若未显式指定，则按 useTs 推导出最终 language。
  const language =
    normalizeString(options.language)?.toLowerCase() === 'javascript' || useTs === false
      ? 'JavaScript'
      : 'TypeScript';

  return {
    projectType: normalizeString(options.projectType) ?? 'spa',
    cssProcessor: normalizeCssProcessor(options.cssProcessor),
    cssTools: normalizeStringArray(options.cssTools),
    lintTools: normalizeStringArray(options.lintTools),
    buildTool: normalizeBuildTool(options.buildTool),
    packageManager: normalizeString(options.packageManager),
    repository: normalizeString(options.repository) ?? '',
    language,
    useTs: language === 'TypeScript',
    skipGit: options.skipGit === true,
    skipInstall: options.skipInstall === true,
    skipInstallSpecified: options.skipInstallSpecified === true,
    port: normalizePort(options.port),
  };
}

export const CSS_PROCESSOR_CHOICES = ['css', 'less', 'sass', 'stylus'];
export const CSS_TOOL_CHOICES = ['tailwindcss'];
export const LINT_TOOL_CHOICES = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'stylelint',
  'commitlint',
  'editorconfig',
];
export const PACKAGE_MANAGER_CHOICES = ['pnpm', 'npm', 'yarn', 'bun'];

function normalizeString(value: unknown) {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function normalizeStringArray(value: unknown) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0);
}

function normalizeCssProcessor(value: unknown): SpaVueTemplateOptions['cssProcessor'] {
  const normalized = normalizeString(value)?.toLowerCase();
  if (
    normalized === 'css' ||
    normalized === 'less' ||
    normalized === 'sass' ||
    normalized === 'stylus'
  ) {
    return normalized;
  }
  return 'css';
}

function normalizeBuildTool(value: unknown): SpaVueTemplateOptions['buildTool'] {
  return normalizeString(value)?.toLowerCase() === 'webpack' ? 'webpack' : 'vite';
}

function normalizePort(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 3000;
}
