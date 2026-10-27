/**
 * §12 验收：同一份 ruff 输出经 **Python bridge** 与 **JS 簇参考解析器** 解析出来的
 * `Diagnostic` 必须深度相等。
 *
 * 两边读同一份 fixture + 同一份 golden：
 * - TS 侧（本文件）：`parseRuffDiagnostics`
 * - Python 侧：`rust/crates/lania-py-bridge/src/lib.rs::ruff_parser_matches_the_shared_golden`
 *
 * 因此任何一边改了字段/取值，都会有一侧先红 —— 这正是「跨语言解析不漂移」的锁。
 */
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { parseRuffDiagnostics } from './parsers.js';

const here = dirname(fileURLToPath(import.meta.url));
// source 布局：ts/packages/node-bridge/src/plugins/lint/<this file>
const fixtures = join(here, '../../../../../../rust/crates/lania-py-bridge/assets/fixtures');

test('ruff parser matches the python bridge golden', () => {
  const raw = readFileSync(join(fixtures, 'ruff-sample.json'), 'utf8');
  const expected = JSON.parse(readFileSync(join(fixtures, 'ruff-diagnostics.json'), 'utf8'));

  assert.deepEqual(parseRuffDiagnostics(raw), expected);
});
