/**
 * lint 子模块共享类型。
 *
 * 这些类型刻意保持“协议优先”：
 * - adaptor/mode 对应请求与结果里的枚举值
 * - `LintRunResult` 对应 bridge 对外暴露的稳定结构
 */
export const LINT_FORMATTER = 'lania.lint.formatter.v1';
export const LINT_NORMALIZER = 'lania.lint.normalizer.v1';

export const LINT_ADAPTORS = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'stylelint',
  'textlint',
] as const;

export type LintAdaptor = (typeof LINT_ADAPTORS)[number];
export type LintMode = 'check' | 'fix';

export type LintRunFile = {
  filePath: string;
  errors: number;
  warnings: number;
};

export type LintRunResult = {
  adaptor: LintAdaptor;
  errors: number;
  warnings: number;
  implementation: 'runtime' | 'fallback';
  formatter: typeof LINT_FORMATTER;
  normalizer: typeof LINT_NORMALIZER;
  files: LintRunFile[];
};

export type LintSummary = {
  errors: number;
  warnings: number;
  files: number;
};

export function isLintAdaptor(value: unknown): value is LintAdaptor {
  return typeof value === 'string' && LINT_ADAPTORS.includes(value as LintAdaptor);
}

export function createLintRunResult(
  adaptor: LintAdaptor,
  implementation: LintRunResult['implementation'],
  files: LintRunFile[],
  errors = files.reduce((count, file) => count + file.errors, 0),
  warnings = files.reduce((count, file) => count + file.warnings, 0),
): LintRunResult {
  return {
    adaptor,
    errors,
    warnings,
    implementation,
    formatter: LINT_FORMATTER,
    normalizer: LINT_NORMALIZER,
    files,
  };
}
