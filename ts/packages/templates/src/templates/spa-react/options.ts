/**
 * spa-react 模板的可选项默认值与选项元信息。
 *
 * 主要导出：normalizeSpaReactOptions、CSS_PROCESSOR_CHOICES、CSS_TOOL_CHOICES、LINT_TOOL_CHOICES、PACKAGE_MANAGER_CHOICES、SpaReactTemplateOptions。
 *
 * 与 spa-vue 的职责类似：
 * - 收口来自 CLI/context 的原始输入
 * - 统一默认值与历史兼容字段
 * - 给 questions/config/dependencies 提供一份稳定的 normalized options
 */
export interface SpaReactTemplateOptions {
  /**
   * 项目类型（用于控制问题项显隐等行为）。
   * 某些非前端 projectType 会跳过样式相关问题。
   */
  projectType: string;
  /** CSS 预处理器类型（决定样式模板与对应 loader 依赖）。 */
  cssProcessor: 'css' | 'less' | 'sass' | 'stylus';
  /** CSS 工具集合，例如 tailwindcss。 */
  cssTools: string[];
  /** 工程化工具集合（eslint/oxlint/prettier/oxfmt/stylelint/commitlint/editorconfig）。 */
  lintTools: string[];
  /** 构建工具（vite/webpack）。 */
  buildTool: 'vite' | 'webpack';
  /** 包管理器；为 null 时通常由 questions.ts 提问。 */
  packageManager: string | null;
  /** 仓库地址（可为空）。 */
  repository: string;
  /** 用户选择的语言偏好。 */
  language: 'TypeScript' | 'JavaScript';
  /** 是否使用 TypeScript。 */
  useTs: boolean;
  /** 是否跳过 git 初始化相关流程。 */
  skipGit: boolean;
  /** 是否跳过依赖安装。 */
  skipInstall: boolean;
  /** 是否显式指定了 skipInstall。 */
  skipInstallSpecified: boolean;
  /** dev server 端口。 */
  port: number;
}

export function normalizeSpaReactOptions(
  options: Record<string, unknown> = {},
): SpaReactTemplateOptions {
  const useTs =
    typeof options.useTs === 'boolean'
      ? options.useTs
      : options.useTypescript !== false && options.language !== 'javascript';
  // React 模板同时兼容 useTs / useTypescript / language 三类输入。
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

function normalizeCssProcessor(value: unknown): SpaReactTemplateOptions['cssProcessor'] {
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

function normalizeBuildTool(value: unknown): SpaReactTemplateOptions['buildTool'] {
  return normalizeString(value)?.toLowerCase() === 'webpack' ? 'webpack' : 'vite';
}

function normalizePort(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 3000;
}
