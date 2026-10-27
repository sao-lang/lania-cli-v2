import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { runLanguageAdaptor } from './language-runners.js';
import {
  parseBlackDiagnostics,
  parseBunBuildDiagnostics,
  parseClippyDiagnostics,
  parseDartAnalyzeDiagnostics,
  parseDenoFmtDiagnostics,
  parseDenoLintDiagnostics,
  parseFlake8Diagnostics,
  parseGolangciDiagnostics,
  parseGoVetDiagnostics,
  parseMypyDiagnostics,
  parseRuffDiagnostics,
} from './parsers.js';
import { fallbackMissingAdaptor, resolveLintTools } from './shared.js';
import { LINT_ADAPTORS } from './types.js';

test('language lint adaptors are registered and selectable', () => {
  for (const adaptor of [
    'ruff',
    'black',
    'mypy',
    'flake8',
    'golangci-lint',
    'go-vet',
    'clippy',
    'dart-analyze',
  ]) {
    assert.ok((LINT_ADAPTORS as readonly string[]).includes(adaptor), adaptor);
  }
  assert.deepEqual(resolveLintTools({ lintTools: ['ruff', 'clippy'] }), ['ruff', 'clippy']);
  assert.match(fallbackMissingAdaptor('ruff').message ?? '', /not installed/);
});

test('language lint parsers produce full diagnostics, not just file+severity', () => {
  // ruff：JSON 已含全部字段，必须保留行号 / 规则号 / 修复建议。
  const ruff = parseRuffDiagnostics(
    JSON.stringify([
      {
        code: 'E501',
        filename: 'src/a.py',
        location: { row: 12, column: 5 },
        end_location: { row: 12, column: 20 },
        message: 'line too long (95 > 88)',
        url: 'https://docs.astral.sh/ruff/rules/line-too-long',
        fix: { applicability: 'safe' },
      },
    ]),
  );
  assert.deepEqual(ruff, [
    {
      tool: 'ruff',
      id: 'E501',
      severity: 'error',
      message: 'line too long (95 > 88)',
      file: 'src/a.py',
      range: { start: { line: 12, col: 5 }, end: { line: 12, col: 20 } },
      url: 'https://docs.astral.sh/ruff/rules/line-too-long',
      fix: { available: true, kind: 'safe' },
    },
  ]);

  // mypy：文本，规则号在末尾方括号里；note 行被丢弃。
  const mypy = parseMypyDiagnostics(
    'a.py:1: error: x  [assignment]\nb.py:2: warning: y\nc.py:3: note: z',
  );
  assert.equal(mypy.length, 2);
  assert.equal(mypy[0].id, 'assignment');
  assert.equal(mypy[0].message, 'x');
  assert.equal(mypy[0].range?.start.line, 1);
  assert.equal(mypy[1].severity, 'warning');

  // flake8：行/列/规则号都在文本里。
  const flake8 = parseFlake8Diagnostics('a.py:1:1: E501 x\nb.py:2:2: W291 y');
  assert.equal(flake8.length, 2);
  assert.equal(flake8[0].id, 'E501');
  assert.equal(flake8[0].severity, 'error');
  assert.equal(flake8[1].id, 'W291');
  assert.equal(flake8[1].severity, 'warning');
  assert.equal(flake8[1].range?.start.col, 2);

  // golangci-lint：JSON 带 linter 名与位置。
  const golangci = parseGolangciDiagnostics(
    JSON.stringify({
      Issues: [
        {
          FromLinter: 'errcheck',
          Severity: 'warning',
          Text: 'unchecked',
          Pos: { Filename: 'main.go', Line: 33, Column: 2 },
        },
      ],
    }),
  );
  assert.equal(golangci.length, 1);
  assert.equal(golangci[0].id, 'errcheck');
  assert.equal(golangci[0].range?.start.line, 33);
  assert.equal(golangci[0].severity, 'warning');

  // go vet：纯文本。
  const govet = parseGoVetDiagnostics('main.go:3:5: something');
  assert.deepEqual(govet[0].range, { start: { line: 3, col: 5 } });
  assert.equal(govet[0].message, 'something');

  // clippy：jsonl。
  const clippy = parseClippyDiagnostics(
    JSON.stringify({
      reason: 'compiler-message',
      message: {
        level: 'warning',
        message: 'unused variable',
        code: { code: 'unused_variables' },
        spans: [{ file_name: 'src/lib.rs', line_start: 7, column_start: 9 }],
      },
    }),
  );
  assert.equal(clippy.length, 1);
  assert.equal(clippy[0].id, 'unused_variables');
  assert.equal(clippy[0].range?.start.line, 7);

  // dart analyze：8 段机器格式。
  const dart = parseDartAnalyzeDiagnostics(
    'ERROR|COMPILE|X|a.dart|1|1|1|boom\nWARNING|LINT|Y|b.dart|2|1|1|warn',
  );
  assert.equal(dart.length, 2);
  assert.equal(dart[0].id, 'X');
  assert.equal(dart[0].message, 'boom');
  assert.equal(dart[1].severity, 'warning');

  // black：只有文件，没有行列号。
  const black = parseBlackDiagnostics('would reformat a.py\nall done');
  assert.equal(black.length, 1);
  assert.equal(black[0].file, 'a.py');
  assert.equal(black[0].range, undefined);
  assert.deepEqual(black[0].fix, { available: true, kind: 'safe' });
});

test('parsers are deterministic and pure (same input => deep-equal output)', () => {
  const sample = JSON.stringify([
    {
      code: 'E501',
      filename: 'src/a.py',
      location: { row: 1, column: 1 },
      end_location: { row: 1, column: 2 },
      message: 'm',
    },
  ]);
  // 「同一份 ruff 输出经不同 bridge 路径解析，Diagnostic 深度相等」的前提就是这个。
  assert.deepEqual(parseRuffDiagnostics(sample), parseRuffDiagnostics(sample));
});

test('missing language tools fall back instead of throwing', async () => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-lang-lint-'));
  try {
    const probe = spawnSync('ruff', ['--version'], { stdio: 'ignore' });
    const available = !probe.error && probe.status === 0;
    const result = await runLanguageAdaptor('ruff', cwd, false);
    assert.ok(result);
    assert.equal(result?.implementation, available ? 'runtime' : 'fallback');
    if (!available) {
      assert.match(result?.message ?? '', /not installed/);
    }
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('deno lint/fmt and bun build adaptors are selectable', () => {
  for (const adaptor of ['deno-lint', 'deno-fmt']) {
    assert.ok((LINT_ADAPTORS as readonly string[]).includes(adaptor), adaptor);
  }
  assert.deepEqual(resolveLintTools({ lintTools: ['deno-lint', 'deno-fmt'] }), [
    'deno-lint',
    'deno-fmt',
  ]);
  // bun 只出现在 §7.3 的 build 诊断里，不是 lint adaptor —— 不应混进 lint 工具集。
  assert.equal((LINT_ADAPTORS as readonly string[]).includes('bun'), false);
});

test('deno lint JSON output keeps rule id and 1-based position', () => {
  const json = JSON.stringify({
    diagnostics: [
      {
        filename: 'src/a.ts',
        range: { start: { line: 12, col: 5 }, end: { line: 12, col: 9 } },
        code: 'no-explicit-any',
        message: '`any` type is not allowed',
      },
    ],
  });
  const diagnostics = parseDenoLintDiagnostics(json);
  assert.deepEqual(diagnostics, [
    {
      tool: 'deno-lint',
      id: 'no-explicit-any',
      severity: 'error',
      message: '`any` type is not allowed',
      file: 'src/a.ts',
      range: { start: { line: 12, col: 5 }, end: { line: 12, col: 9 } },
      fix: { available: true, kind: 'safe' },
    },
  ]);
});

test('deno lint v1 text output is parsed instead of silently dropped', () => {
  const text = [
    'error[no-unused-vars]: `x` is never used',
    ' --> /repo/main.ts:3:7',
    '  |',
    '3 |   const x = 1;',
  ].join('\n');
  const diagnostics = parseDenoLintDiagnostics(text);
  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0].id, 'no-unused-vars');
  assert.equal(diagnostics[0].file, '/repo/main.ts');
  assert.deepEqual(diagnostics[0].range?.start, { line: 3, col: 7 });
});

test('deno fmt has no line info, so it emits file-level diagnostics', () => {
  const output = 'from /repo/main.ts:\n  \n\nfrom /repo/util.ts:\n';
  const diagnostics = parseDenoFmtDiagnostics(output);
  assert.deepEqual(
    diagnostics.map((item) => item.file),
    ['/repo/main.ts', '/repo/util.ts'],
  );
  for (const diagnostic of diagnostics) {
    assert.equal(diagnostic.tool, 'deno-fmt');
    assert.equal(diagnostic.range, undefined);
    assert.deepEqual(diagnostic.fix, { available: true, kind: 'safe' });
  }
});

test('bun build stderr is parsed into located diagnostics', () => {
  // 真实 `bun 1.3.0 build` 失败输出（stderr），见 PR 说明。
  const stderr = [
    "1 | import './does-not-exist.ts';",
    '           ^',
    'error: Could not resolve: "./does-not-exist.ts"',
    '    at /repo/entry.ts:1:8',
    '',
  ].join('\n');
  const diagnostics = parseBunBuildDiagnostics(stderr);
  assert.deepEqual(diagnostics, [
    {
      tool: 'bun',
      severity: 'error',
      message: 'Could not resolve: "./does-not-exist.ts"',
      file: '/repo/entry.ts',
      range: { start: { line: 1, col: 8 } },
    },
  ]);
});

test('bun build reports every error, not just the last one', () => {
  const stderr = [
    '1 | const b = ;',
    '              ^',
    'error: Unexpected ;',
    '    at /repo/b2.ts:1:11',
    '',
    '1 | const a = ;',
    '              ^',
    'error: Unexpected ;',
    '    at /repo/b1.ts:1:11',
  ].join('\n');
  const diagnostics = parseBunBuildDiagnostics(stderr);
  assert.equal(diagnostics.length, 2);
  assert.deepEqual(
    diagnostics.map((item) => item.file),
    ['/repo/b2.ts', '/repo/b1.ts'],
  );
});
