/**
 * 各 lint 工具的原始输出 → 统一 `Diagnostic`（§7.3）。
 *
 * 设计取舍：
 * - **每个解析器都是纯函数**（字符串进、`Diagnostic[]` 出），
 *   这样才能断言「同一份 ruff 输出经不同路径解析，诊断深度相等」。
 * - 解析器**不做聚合**：`files`（按文件计数）由 `aggregateDiagnostics` 派生，
 *   避免两处各算一遍导致漂移。
 * - 结构化输出（JSON）优先走 JSON；文本输出用尽量严格的正则，
 *   宁可漏报也不要误报（漏报会在计数上表现为 0 条，容易发现）。
 */
import { asRecord } from '../../core/runtime.js';
import { makeDiagnostic, type Diagnostic, type DiagnosticSeverity } from '../diagnostics.js';

function severityFromText(value: unknown): DiagnosticSeverity {
  // ESLint: 2 = error / 1 = warning；golangci / clippy 传字符串。
  if (value === 2 || value === 'error' || value === 'deny') return 'error';
  if (value === 1 || value === 'warning' || value === 'warn') return 'warning';
  if (value === 'info' || value === 'note') return 'info';
  if (value === 'hint') return 'hint';
  return 'error';
}

function num(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function str(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

/** ruff `--output-format json`：JSON 已含全部字段，只需改名。 */
export function parseRuffDiagnostics(output: string): Diagnostic[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(output || '[]');
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) {
    return [];
  }
  return parsed.flatMap((item) => {
    const record = asRecord(item);
    const file = str(record.filename) ?? str(record.file);
    if (!file) return [];
    const location = asRecord(record.location);
    const endLocation = asRecord(record.end_location);
    const fix = record.fix;
    return [
      makeDiagnostic('ruff', {
        id: str(record.code),
        severity: 'error',
        message: str(asRecord(record.message).message) ?? str(record.message) ?? '',
        file,
        line: num(location.row),
        col: num(location.column),
        endLine: num(endLocation.row),
        endCol: num(endLocation.column),
        url: str(record.url),
        ...(fix !== null && fix !== undefined
          ? {
              fixable: true,
              fixKind: asRecord(fix).applicability === 'unsafe' ? 'unsafe' : 'safe',
            }
          : {}),
      }),
    ];
  });
}

/** black `--check`：只有「would reformat <file>」，没有行列号。 */
export function parseBlackDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const marker = 'would reformat ';
    const index = line.indexOf(marker);
    if (index < 0) continue;
    const file = line.slice(index + marker.length).trim();
    if (!file) continue;
    diagnostics.push(
      makeDiagnostic('black', {
        severity: 'warning',
        message: 'would reformat',
        file,
        fixable: true,
      }),
    );
  }
  return diagnostics;
}

/**
 * mypy 文本：`file:line: severity: message  [code]`
 *
 * `note:` 行是补充说明，不算独立诊断（否则一条错误会变成三条计数）。
 */
export function parseMypyDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^(.+?):(\d+): (error|warning|note): (.*)$/.exec(line);
    if (!match || match[3] === 'note') {
      continue;
    }
    const body = match[4];
    // mypy 把规则号放在消息末尾：`...  [assignment]`
    const idMatch = /\s\[([a-z0-9-]+)\]$/.exec(body);
    diagnostics.push(
      makeDiagnostic('mypy', {
        id: idMatch ? idMatch[1] : null,
        severity: match[3] === 'error' ? 'error' : 'warning',
        message: idMatch ? body.slice(0, idMatch.index).trim() : body.trim(),
        file: match[1],
        line: Number(match[2]),
        col: 1,
      }),
    );
  }
  return diagnostics;
}

/** flake8 文本：`file:line:col: CODE message` */
export function parseFlake8Diagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^(.+?):(\d+):(\d+): ([A-Z]\d+) (.*)$/.exec(line);
    if (!match) continue;
    diagnostics.push(
      makeDiagnostic('flake8', {
        id: match[4],
        severity: match[4].startsWith('W') ? 'warning' : 'error',
        message: match[5].trim(),
        file: match[1],
        line: Number(match[2]),
        col: Number(match[3]),
      }),
    );
  }
  return diagnostics;
}

/** golangci-lint `--out-format json`（旧版数组 / 新版 `{Issues:[...]}` 都认）。 */
export function parseGolangciDiagnostics(output: string): Diagnostic[] {
  try {
    const parsed = JSON.parse(output);
    const issues = Array.isArray(parsed)
      ? parsed
      : Array.isArray(asRecord(parsed).Issues)
        ? (asRecord(parsed).Issues as unknown[])
        : [];
    const diagnostics: Diagnostic[] = [];
    for (const issue of issues) {
      const record = asRecord(issue);
      const position = asRecord(record.Pos ?? record.pos);
      const file = str(position.Filename) ?? str(position.filename);
      if (!file) continue;
      const replacement = record.Replacement ?? asRecord(record.replacement);
      const hasReplacement =
        typeof replacement === 'string' && replacement.length > 0
          ? true
          : Boolean(str(asRecord(replacement).New) ?? str(asRecord(replacement).new));
      diagnostics.push(
        makeDiagnostic('golangci-lint', {
          id: str(record.FromLinter) ?? str(asRecord(record).fromLinter),
          severity: severityFromText(record.Severity ?? record.severity),
          message: str(record.Text) ?? str(record.text) ?? '',
          file,
          line: num(position.Line) ?? num(position.line),
          col: num(position.Column) ?? num(position.column),
          fixable: hasReplacement,
        }),
      );
    }
    return diagnostics;
  } catch {
    // 非 JSON：回落到 `file:line:col: message` 文本形态。
    const diagnostics: Diagnostic[] = [];
    for (const line of output.split(/\r?\n/)) {
      const match = /^(.+?):(\d+):(\d+): (.*)$/.exec(line);
      if (!match) continue;
      diagnostics.push(
        makeDiagnostic('golangci-lint', {
          severity: 'error',
          message: match[4].trim(),
          file: match[1],
          line: Number(match[2]),
          col: Number(match[3]),
        }),
      );
    }
    return diagnostics;
  }
}

/** `go vet` 文本：`file.go:line:col: message` */
export function parseGoVetDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^(.+?\.go):(\d+):(\d+): (.*)$/.exec(line);
    if (!match) continue;
    diagnostics.push(
      makeDiagnostic('go-vet', {
        severity: 'error',
        message: match[4].trim(),
        file: match[1],
        line: Number(match[2]),
        col: Number(match[3]),
      }),
    );
  }
  return diagnostics;
}

/** clippy `--message-format=json`（jsonl，每行一个 cargo 消息）。 */
export function parseClippyDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let record: Record<string, unknown>;
    try {
      record = asRecord(JSON.parse(line));
    } catch {
      continue;
    }
    if (record.reason !== 'compiler-message') continue;
    const message = asRecord(record.message);
    const level = severityFromText(message.level);
    const spans = Array.isArray(message.spans) ? message.spans : [];
    const span = spans.map(asRecord).find((item) => typeof item.file_name === 'string');
    const code = asRecord(message.code);
    const suggested = spans
      .map(asRecord)
      .map((item) => item.suggested_replacement)
      .find((value) => typeof value === 'string');
    diagnostics.push(
      makeDiagnostic('clippy', {
        id: str(code.code),
        severity: level,
        message: str(message.message) ?? '',
        file: span ? str(span.file_name) : null,
        line: span ? num(span.line_start) : null,
        col: span ? num(span.column_start) : null,
        endLine: span ? num(span.line_end) : null,
        endCol: span ? num(span.column_end) : null,
        fixable: typeof suggested === 'string',
      }),
    );
  }
  return diagnostics;
}

/** `dart analyze --format machine`：`SEVERITY|TYPE|CODE|file|line|col|len|message` */
export function parseDartAnalyzeDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const fields = line.split('|');
    if (fields.length < 8) continue;
    const [severityRaw, , code, file, lineNo, col, , ...messageParts] = fields;
    if (!file) continue;
    const severity: DiagnosticSeverity =
      severityRaw === 'ERROR' ? 'error' : severityRaw === 'WARNING' ? 'warning' : 'info';
    diagnostics.push(
      makeDiagnostic('dart-analyze', {
        id: code || null,
        severity,
        message: messageParts.join('|').trim(),
        file,
        line: Number(lineNo),
        col: Number(col),
      }),
    );
  }
  return diagnostics;
}

/** oxlint JSON（形态随版本变化，用宽松收集）。 */
export function parseOxlintDiagnostics(output: string): Diagnostic[] {
  if (!output.trim()) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return [];
  }
  return collectOxlint(parsed, undefined);
}

function collectOxlint(value: unknown, inheritedFile?: string): Diagnostic[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => collectOxlint(item, inheritedFile));
  }
  if (!value || typeof value !== 'object') {
    return [];
  }
  const record = value as Record<string, unknown>;
  const file =
    str(record.filePath) ??
    str(record.filename) ??
    str(record.path) ??
    str(record.source) ??
    inheritedFile;

  for (const key of ['messages', 'diagnostics', 'results'] as const) {
    if (Array.isArray(record[key])) {
      return (record[key] as unknown[]).flatMap((item) => collectOxlint(item, file));
    }
  }

  const severity = record.severity ?? record.level ?? record.kind;
  const normalized =
    severity === 'error' || severity === 'deny' || severity === 2
      ? 'error'
      : severity === 'warning' || severity === 'warn' || severity === 1
        ? 'warning'
        : null;
  if (!normalized || !file) {
    return [];
  }
  const labels = Array.isArray(record.labels) ? record.labels.map(asRecord) : [];
  const primary = labels.find((item) => item.span) ?? labels[0];
  const span = asRecord(primary?.span);
  const start = asRecord(span.start);
  const end = asRecord(span.end);
  const help = asRecord(record.help);
  return [
    makeDiagnostic('oxlint', {
      id: str(record.code) ?? str(record.ruleId) ?? str(record.rule_id),
      severity: normalized,
      message: str(record.message) ?? '',
      file,
      line: num(start.line) !== null ? (num(start.line) as number) + 1 : null,
      col: num(start.column) !== null ? (num(start.column) as number) + 1 : null,
      endLine: num(end.line) !== null ? (num(end.line) as number) + 1 : null,
      endCol: num(end.column) !== null ? (num(end.column) as number) + 1 : null,
      ...(help ? { fixable: true } : {}),
    }),
  ];
}

/** ESLint API 的 `messages[]` → `Diagnostic[]`。 */
export function diagnosticsFromEslintMessages(filePath: string, messages: unknown[]): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const message of messages) {
    const record = asRecord(message);
    diagnostics.push(
      makeDiagnostic('eslint', {
        id: str(record.ruleId),
        severity: severityFromText(record.severity),
        message: str(record.message) ?? '',
        file: filePath,
        line: num(record.line),
        col: num(record.column),
        fixable: record.fix !== undefined && record.fix !== null,
      }),
    );
  }
  return diagnostics;
}

/**
 * `deno lint --json`（v2+）：
 * `{"diagnostics":[{"filename","range":{"start":{"line","col"},"end":{...}},"code","message"}]}`
 *
 * 行列号是 **1-based**，与 `Diagnostic` 一致，不做偏移（这一点与 oxlint 的 0-based 相反）。
 */
export function parseDenoLintDiagnostics(output: string): Diagnostic[] {
  const trimmed = output.trim();
  if (!trimmed) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    // v1 没有 `--json`：回落到文本形态 `error[code]: msg\n  --> file:line:col`。
    return parseDenoLintText(trimmed);
  }
  const entries = asRecord(parsed).diagnostics;
  if (!Array.isArray(entries)) {
    return parseDenoLintText(trimmed);
  }
  const diagnostics: Diagnostic[] = [];
  for (const entry of entries) {
    const record = asRecord(entry);
    const file = str(record.filename) ?? str(record.file);
    if (!file) continue;
    const range = asRecord(record.range);
    const start = asRecord(range.start);
    const end = asRecord(range.end);
    diagnostics.push(
      makeDiagnostic('deno-lint', {
        id: str(record.code),
        // deno lint 只报规则违规，没有 severity 字段；规则违反按 error 处理（与退出码一致）。
        severity: severityFromText(record.severity ?? 'error'),
        message: str(record.message) ?? '',
        file,
        line: num(start.line),
        col: num(start.col),
        endLine: num(end.line),
        endCol: num(end.col),
        fixable: true,
      }),
    );
  }
  return diagnostics;
}

/** deno lint 文本形态（v1）：`error[no-explicit-any]: \`any\` type is not allowed` + `--> file:l:c` */
function parseDenoLintText(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const header = /^(?:error|warning)\[([^\]]+)\]:\s*(.*)$/;
  for (const block of output.split(/\n\s*\n/)) {
    const lines = block.split(/\r?\n/);
    const title = header.exec(lines[0]?.trim() ?? '');
    if (!title) continue;
    const location = lines
      .map((line) => /^\s*-->\s+(.+?):(\d+):(\d+)\s*$/.exec(line))
      .find((match) => match !== null);
    if (!location) continue;
    diagnostics.push(
      makeDiagnostic('deno-lint', {
        id: title[1],
        severity: 'error',
        message: title[2].trim(),
        file: location[1],
        line: Number(location[2]),
        col: Number(location[3]),
        fixable: true,
      }),
    );
  }
  return diagnostics;
}

/**
 * `deno fmt --check`：只打印待格式化文件清单（或 diff），**没有行列号**。
 *
 * 输出形如：
 * ```text
 * from /repo/main.ts:
 *  ...
 *
 * Checked 3 files
 * ```
 * 因此只产出文件级诊断，`fix.available` 恒为 true（`deno fmt` 可直接改）。
 */
export function parseDenoFmtDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const seen = new Set<string>();
  const push = (file: string) => {
    const trimmed = file.trim();
    if (!trimmed || seen.has(trimmed)) return;
    seen.add(trimmed);
    diagnostics.push(
      makeDiagnostic('deno-fmt', {
        severity: 'warning',
        message: 'not formatted',
        file: trimmed,
        fixable: true,
      }),
    );
  };
  for (const line of output.split(/\r?\n/)) {
    const fromFile = /^from (.+):\s*$/.exec(line.trim());
    if (fromFile) {
      push(fromFile[1]);
      continue;
    }
    // 无 diff 引擎时的紧凑形态：每行一个文件路径。
    if (/^\S+\.(?:[cm]?[jt]sx?|json|jsonc|md)$/.test(line.trim())) {
      push(line);
    }
  }
  return diagnostics;
}

/**
 * `bun build` 失败输出（stderr）：
 * ```text
 * 1 | import './missing.ts';
 *           ^
 * error: Could not resolve: "./missing.ts"
 *     at /repo/entry.ts:1:8
 * ```
 * 一段一条诊断；`at <file>:<line>:<col>` 是定位行，缺失时退化为文件级诊断。
 */
export function parseBunBuildDiagnostics(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const lines = output.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const header = /^\s*error:\s*(.+?)\s*$/.exec(lines[index]);
    if (!header) continue;
    let file: string | null = null;
    let line: number | null = null;
    let col: number | null = null;
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const location = /^\s*at\s+(.+?):(\d+):(\d+)\s*$/.exec(lines[cursor]);
      if (location) {
        file = location[1];
        line = Number(location[2]);
        col = Number(location[3]);
        index = cursor;
        break;
      }
      // 下一条 error 说明这段已结束，且没有定位信息。
      if (/^\s*error:/.test(lines[cursor])) {
        break;
      }
    }
    diagnostics.push(
      makeDiagnostic('bun', {
        severity: 'error',
        message: header[1],
        file,
        line,
        col,
      }),
    );
  }
  return diagnostics;
}
