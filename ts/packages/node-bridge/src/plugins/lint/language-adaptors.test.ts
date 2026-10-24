import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  parseBlackOutput,
  parseClippyOutput,
  parseDartAnalyzeOutput,
  parseFlake8Output,
  parseGolangciOutput,
  parseGoVetOutput,
  parseMypyOutput,
  parseRuffOutput,
  runLanguageAdaptor,
} from './language-runners.js';
import { resolveLintTools } from './shared.js';
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
});

test('language lint parsers normalize tool output', () => {
  assert.deepEqual(parseRuffOutput(JSON.stringify([{ filename: 'a.py' }, { filename: 'b.py' }])), [
    { filePath: 'a.py', severity: 'error' },
    { filePath: 'b.py', severity: 'error' },
  ]);

  assert.deepEqual(parseMypyOutput('a.py:1: error: x\nb.py:2: warning: y\nc.py:3: note: z'), [
    { filePath: 'a.py', severity: 'error' },
    { filePath: 'b.py', severity: 'warning' },
  ]);

  assert.deepEqual(parseFlake8Output('a.py:1:1: E501 x\nb.py:2:2: W291 y'), [
    { filePath: 'a.py', severity: 'error' },
    { filePath: 'b.py', severity: 'warning' },
  ]);

  assert.deepEqual(
    parseGolangciOutput(
      JSON.stringify({
        Issues: [
          { Severity: 'warning', Pos: { Filename: 'main.go' } },
          { Pos: { Filename: 'other.go' } },
        ],
      }),
    ),
    [
      { filePath: 'main.go', severity: 'warning' },
      { filePath: 'other.go', severity: 'error' },
    ],
  );

  assert.deepEqual(parseGoVetOutput('main.go:3:5: something'), [
    { filePath: 'main.go', severity: 'error' },
  ]);

  assert.deepEqual(
    parseClippyOutput(
      JSON.stringify({
        reason: 'compiler-message',
        message: { level: 'warning', spans: [{ file_name: 'src/lib.rs' }] },
      }),
    ),
    [{ filePath: 'src/lib.rs', severity: 'warning' }],
  );

  assert.deepEqual(
    parseDartAnalyzeOutput('ERROR|COMPILE|X|a.dart|1|1|1|boom\nWARNING|LINT|Y|b.dart|2|1|1|warn'),
    [
      { filePath: 'a.dart', severity: 'error' },
      { filePath: 'b.dart', severity: 'warning' },
    ],
  );

  assert.deepEqual(parseBlackOutput('would reformat a.py\nall done'), [
    { filePath: 'a.py', severity: 'warning' },
  ]);
});

test('missing language tools fall back instead of throwing', async () => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-lang-lint-'));
  try {
    const probe = spawnSync('ruff', ['--version'], { stdio: 'ignore' });
    const available = !probe.error && probe.status === 0;
    const result = await runLanguageAdaptor('ruff', cwd, false);
    assert.ok(result);
    assert.equal(result?.implementation, available ? 'runtime' : 'fallback');
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});
