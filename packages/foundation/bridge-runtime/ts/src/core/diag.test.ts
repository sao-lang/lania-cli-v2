import assert from 'node:assert/strict';
import test from 'node:test';

import type { BridgeEvent } from '../protocol/events.js';
import {
  createDiagEmitter,
  createLineSplitter,
  inferLevel,
  nextDiagSeq,
  resetDiagSequence,
} from './diag.js';

test('seq is strictly increasing across stdout and stderr', () => {
  resetDiagSequence();
  const emitted: BridgeEvent[] = [];
  const emit = (event: BridgeEvent) => emitted.push(event);
  const stdout = createDiagEmitter(emit, { channel: 'build', tool: 'go' });
  const stderr = createDiagEmitter(emit, { channel: 'build', tool: 'go' });

  // 交错写入：这是真实情况（两个独立异步流）。
  stdout('compiling', 'stdout');
  stderr('warning: unused', 'stderr');
  stdout('done', 'stdout');

  const seqs = emitted.map((event) => (event.params as { seq: number }).seq);
  assert.deepEqual(seqs, [1, 2, 3]);
  // 严格递增，且无重复 —— 这是上层合并两条流的唯一依据。
  for (let index = 1; index < seqs.length; index += 1) {
    assert.ok(seqs[index] > seqs[index - 1], `seq must increase: ${seqs}`);
  }
});

test('emits channel and source for every line', () => {
  resetDiagSequence();
  const emitted: BridgeEvent[] = [];
  createDiagEmitter((event) => emitted.push(event), {
    channel: 'app',
    tool: 'go',
    source: 'runtime',
  })('GET /api/user 200', 'stdout');

  assert.equal(emitted.length, 1);
  const params = emitted[0].params as Record<string, unknown>;
  assert.equal(emitted[0].method, 'event.diag');
  assert.equal(params.channel, 'app');
  assert.equal(params.source, 'runtime');
  assert.equal(params.tool, 'go');
  assert.equal(params.text, 'GET /api/user 200');
  assert.equal(typeof params.ts, 'number');
});

test('stderr is not blanket-mapped to warning', () => {
  resetDiagSequence();
  const emitted: BridgeEvent[] = [];
  const emitDiag = createDiagEmitter((event) => emitted.push(event), {
    channel: 'build',
    tool: 'go',
  });
  emitDiag('vite v5 ready in 300 ms', 'stderr');

  const params = emitted[0].params as { level: string };
  // 旧实现会把所有 stderr 标成 warning —— 这正是 §7.2 要禁止的。
  assert.equal(params.level, 'info');
});

test('infers real errors from stderr content', () => {
  assert.equal(inferLevel('error: cannot find module', 'stderr'), 'error');
  assert.equal(inferLevel('ERROR something broke', 'stdout'), 'error');
  assert.equal(inferLevel('warning: deprecated API', 'stdout'), 'warn');
  assert.equal(inferLevel('panic: runtime error', 'stderr'), 'error');
  assert.equal(inferLevel('building...', 'stderr'), 'info');
});

test('explicit level wins over inference', () => {
  resetDiagSequence();
  const emitted: BridgeEvent[] = [];
  const emitDiag = createDiagEmitter((event) => emitted.push(event), {
    channel: 'lint',
    tool: 'ruff',
  });
  emitDiag('plain text', 'stdout', 'debug');
  assert.equal((emitted[0].params as { level: string }).level, 'debug');
});

test('no emit callback is a no-op instead of throwing', () => {
  resetDiagSequence();
  const emitDiag = createDiagEmitter(undefined, { channel: 'dev' });
  assert.doesNotThrow(() => emitDiag('line', 'stdout'));
});

test('blank lines are dropped', () => {
  resetDiagSequence();
  const emitted: BridgeEvent[] = [];
  const emitDiag = createDiagEmitter((event) => emitted.push(event), { channel: 'build' });
  emitDiag('   ', 'stdout');
  emitDiag('', 'stderr');
  assert.equal(emitted.length, 0);
});

test('line splitter keeps partial lines and flushes the tail', () => {
  const lines: string[] = [];
  const splitter = createLineSplitter((line) => lines.push(line));

  splitter.push('compiling foo');
  splitter.push('\ncompiling ba');
  splitter.push('r\nno-newline-tail');
  assert.deepEqual(lines, ['compiling foo', 'compiling bar']);

  // 进程退出时最后一行可能没有换行符 —— 必须 flush 否则日志丢失。
  splitter.flush();
  assert.deepEqual(lines, ['compiling foo', 'compiling bar', 'no-newline-tail']);
});

test('nextDiagSeq is monotonic and reset is test-only', () => {
  resetDiagSequence();
  assert.equal(nextDiagSeq(), 1);
  assert.equal(nextDiagSeq(), 2);
  resetDiagSequence();
  assert.equal(nextDiagSeq(), 1);
});
