/**
 * 统一诊断模型（跨语言同构）。
 *
 * 三层结构（见 `docs/design/multi-runtime-bridge.zh-CN.md` §7.3）：
 * ① 原始流（`event.diag`）→ ② 结构化诊断（`Diagnostic`）→ ③ 统一结果（`RunResult`，Rust 侧聚合）。
 *
 * 「同一份 ruff 输出经 Python bridge 与 Rust 回落路径解析，`Diagnostic` 深度相等」
 * 是本模型的验收标准 —— 所以字段名、可选性都必须与 Rust `lania_node_bridge::Diagnostic` 一致。
 */

export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint';

export interface DiagnosticPosition {
  line: number;
  col: number;
}

export interface DiagnosticRange {
  start: DiagnosticPosition;
  end?: DiagnosticPosition;
}

export interface DiagnosticFix {
  available: boolean;
  /** `safe` / `unsafe` / `none` */
  kind: 'safe' | 'unsafe' | 'none';
}

export interface Diagnostic {
  tool: string;
  /** 规则号 / 检查名；缺失表示无规则号的通用错误（如编译失败）。 */
  id?: string;
  severity: DiagnosticSeverity;
  message: string;
  /** 相对 cwd 的路径；缺失表示文件级诊断（如 crate 级告警）。 */
  file?: string;
  range?: DiagnosticRange;
  url?: string;
  fix?: DiagnosticFix;
}

/** 去重键：`(tool, file, line, col, id)`。 */
export function diagnosticKey(diagnostic: Diagnostic): string {
  return [
    diagnostic.tool,
    diagnostic.file ?? '',
    diagnostic.range?.start.line ?? 0,
    diagnostic.range?.start.col ?? 0,
    diagnostic.id ?? '',
  ].join('\u0000');
}

const SEVERITY_ORDER: Record<DiagnosticSeverity, number> = {
  error: 0,
  warning: 1,
  info: 2,
  hint: 3,
};

/**
 * 去重 + 排序（与 Rust `normalize_diagnostics` 对齐）：
 * `file` 升序 → `line` 升序 → `col` 升序 → `severity`（error 优先）→ `id`。
 */
export function normalizeDiagnostics(diagnostics: Diagnostic[]): Diagnostic[] {
  const seen = new Set<string>();
  const deduped = diagnostics.filter((diagnostic) => {
    const key = diagnosticKey(diagnostic);
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
  return deduped.sort((left, right) => {
    const byFile = (left.file ?? '').localeCompare(right.file ?? '');
    if (byFile !== 0) return byFile;
    const leftLine = left.range?.start.line ?? 0;
    const rightLine = right.range?.start.line ?? 0;
    if (leftLine !== rightLine) return leftLine - rightLine;
    const leftCol = left.range?.start.col ?? 0;
    const rightCol = right.range?.start.col ?? 0;
    if (leftCol !== rightCol) return leftCol - rightCol;
    const bySeverity = SEVERITY_ORDER[left.severity] - SEVERITY_ORDER[right.severity];
    if (bySeverity !== 0) return bySeverity;
    return (left.id ?? '').localeCompare(right.id ?? '');
  });
}

export interface DiagnosticCounts {
  error: number;
  warning: number;
  info: number;
  hint: number;
}

export function countDiagnostics(diagnostics: Diagnostic[]): DiagnosticCounts {
  const counts: DiagnosticCounts = { error: 0, warning: 0, info: 0, hint: 0 };
  for (const diagnostic of diagnostics) {
    counts[diagnostic.severity] += 1;
  }
  return counts;
}

/**
 * 由单个 lint adaptor 的原始输出构造诊断。
 *
 * `tool` 是 adaptor id（`ruff` / `mypy` / …），这样同一位置被多个工具报警时
 * 渲染层可以用标签区分，而不是被合并掉。
 */
export function makeDiagnostic(
  tool: string,
  input: {
    id?: string | null;
    severity: DiagnosticSeverity;
    message: string;
    file?: string | null;
    line?: number | null;
    col?: number | null;
    endLine?: number | null;
    endCol?: number | null;
    url?: string | null;
    fixable?: boolean;
    fixKind?: DiagnosticFix['kind'];
  },
): Diagnostic {
  const diagnostic: Diagnostic = {
    tool,
    severity: input.severity,
    message: input.message,
  };
  if (input.id) {
    diagnostic.id = input.id;
  }
  if (input.file) {
    diagnostic.file = input.file;
  }
  if (typeof input.line === 'number') {
    diagnostic.range = {
      start: { line: input.line, col: typeof input.col === 'number' ? input.col : 1 },
      ...(typeof input.endLine === 'number'
        ? {
            end: {
              line: input.endLine,
              col: typeof input.endCol === 'number' ? input.endCol : (input.col ?? 1),
            },
          }
        : {}),
    };
  }
  if (input.url) {
    diagnostic.url = input.url;
  }
  if (input.fixable !== undefined) {
    diagnostic.fix = { available: input.fixable, kind: input.fixKind ?? 'safe' };
  }
  return diagnostic;
}
