/**
 * bridge 协议契约相关回归测试。
 *
 * 关键点：
 * - 包含文件系统读写/路径解析
 */
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createHandshakeResponse } from '../index.js';
import { lintPlugin } from './lint.js';

test('handshake exposes the current public method and event contract', () => {
  const response = createHandshakeResponse({
    protocolVersion: '0.1.0',
    transport: 'stdio',
    encoding: 'json',
    hostName: 'lania-host',
  });

  assert.deepEqual(response.methods, [
    'bridge.ping',
    'bridge.shutdown',
    'bridge.metrics',
    'bridge.subscribe',
    'bridge.heartbeat',
    'plugins.resolve',
    'config.loadLan',
    'config.loadTool',
    'commands.resolveDynamic',
    'command.invokeDynamic',
    'hooks.invoke',
    'hooks.invokeInline',
    'compiler.dev',
    'compiler.build',
    'compiler.stop',
    'product.generate',
    'product.inspect',
    'product.build',
    'product.pack',
    'product.publish',
    'lint.run',
    'system.listCommands',
    'template.list',
    'template.getQuestions',
    'template.getDependencies',
    'template.getOutputTasks',
    'template.render',
    'addTemplate.render',
    'commitizen.run',
    'commitlint.run',
  ]);

  assert.deepEqual(response.events, [
    'event.ready',
    'event.log',
    'event.progress',
    'event.dev_url',
    'event.build_asset',
    'event.compiler_start',
    'event.compiler_status',
    'event.compiler_server_ready',
    'event.compiler_asset',
    'event.compiler_issue',
    'event.compiler_watch_change',
    'event.compiler_done',
    'event.lint_start',
    'event.lint_file',
    'event.lint_result',
    'event.lint_summary',
    'event.watch_change',
    'event.shutdown',
    'event.heartbeat',
  ]);
});

test('lint.run keeps formatter, normalizer, and lint event schema stable', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-contract-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const handled = await lintPlugin.handle('lint.run', {
    cwd,
    linters: ['eslint', 'oxlint', 'prettier', 'oxfmt'],
    mode: 'check',
    concurrency: 2,
    fix: false,
  });

  assert.ok(handled);
  assert.equal(handled.result.accepted, true);
  assert.equal(handled.result.formatter, 'lania.lint.formatter.v1');
  assert.equal(handled.result.normalizer, 'lania.lint.normalizer.v1');
  assert.equal(handled.result.mode, 'check');
  assert.equal(handled.result.concurrency, 2);
  assert.equal(typeof handled.result.summaryText, 'string');
  assert.equal(typeof handled.result.summaryByAdaptor.eslint, 'object');
  assert.equal(typeof handled.result.summaryByAdaptor.oxlint, 'object');
  assert.equal(typeof handled.result.summaryByAdaptor.oxfmt, 'object');
  assert.equal(typeof handled.result.resultsByAdaptor.eslint, 'object');
  assert.equal(typeof handled.result.resultsByAdaptor.oxlint, 'object');
  assert.equal(typeof handled.result.resultsByAdaptor.prettier, 'object');
  assert.equal(typeof handled.result.resultsByAdaptor.oxfmt, 'object');

  const eventMethods = handled.events.map((event) => event.method);
  assert.ok(eventMethods.includes('event.lint_start'));
  assert.ok(eventMethods.includes('event.lint_file'));
  assert.ok(eventMethods.includes('event.lint_result'));
  assert.ok(eventMethods.includes('event.lint_summary'));
});
