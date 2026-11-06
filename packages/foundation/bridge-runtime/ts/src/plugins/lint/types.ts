/**
 * lint 子模块共享类型。
 *
 * 这些类型刻意保持“协议优先”：
 * - adaptor/mode 对应请求与结果里的枚举值
 * - `LintRunResult` 对应 bridge 对外暴露的稳定结构
 */
import type { Diagnostic } from '../diagnostics.js';

export const LINT_FORMATTER = 'lania.lint.formatter.v1';
export const LINT_NORMALIZER = 'lania.lint.normalizer.v1';

export const LINT_ADAPTORS = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'stylelint',
  'textlint',
  'ruff',
  'black',
  'mypy',
  'flake8',
  'golangci-lint',
  'go-vet',
  'clippy',
  'dart-analyze',
  'deno-lint',
  'deno-fmt',
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
  /**
   * 结构化诊断（含行号 / 规则号 / 修复建议）。
   *
   * `files` 是从这里**派生**的按文件计数，用于兼容旧字段；
   * 新增消费者应直接读 `diagnostics`。
   */
  diagnostics: Diagnostic[];
  /** 工具缺失或运行失败时的可读原因，供终端统一日志展示。 */
  message?: string;
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
  message?: string,
  diagnostics: Diagnostic[] = [],
): LintRunResult {
  return {
    adaptor,
    errors,
    warnings,
    implementation,
    formatter: LINT_FORMATTER,
    normalizer: LINT_NORMALIZER,
    files,
    diagnostics,
    ...(message ? { message } : {}),
  };
}

/** 由诊断派生按文件计数（保持旧字段可用，避免两处各算一遍）。 */
export function filesFromDiagnostics(diagnostics: Diagnostic[]): LintRunFile[] {
  const byFile = new Map<string, LintRunFile>();
  for (const diagnostic of diagnostics) {
    const filePath = diagnostic.file ?? '.';
    const entry = byFile.get(filePath) ?? { filePath, errors: 0, warnings: 0 };
    if (diagnostic.severity === 'error') {
      entry.errors += 1;
    } else {
      entry.warnings += 1;
    }
    byFile.set(filePath, entry);
  }
  return [...byFile.values()];
}
