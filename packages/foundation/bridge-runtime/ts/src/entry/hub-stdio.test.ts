/**
 * Hub endpoint 能力声明的回归测试。
 *
 * Hub 按完整 capability 精确路由（hub.rs route_request），端点必须 announce
 * 完整 method（如 compiler.dev），而非命名空间前缀（compiler），否则真实业务无法承接。
 */
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { endpointCapabilities, makeHubHooks } from './hub-stdio.js';

test('hub endpoint announces full method names, not namespace prefixes', async () => {
  const capabilities = await endpointCapabilities();
  assert.ok(capabilities.length > 0, '应至少声明一个内建切片方法');
  for (const capability of capabilities) {
    assert.ok(
      capability.includes('.'),
      `capability "${capability}" 必须是含命名空间的完整 method，Hub 才能精确路由`,
    );
  }
  assert.ok(capabilities.includes('compiler.dev'), '应包含 compiler.dev（而非只有 compiler 前缀）');
  // 内建方法必须由 Node 端点自己声明，否则 Hub 单轨下 handshake/metrics/shutdown 无 provider。
  for (const builtin of ['bridge.ping', 'bridge.metrics', 'bridge.shutdown', 'plugins.resolve']) {
    assert.ok(capabilities.includes(builtin), `应声明内建方法 ${builtin}`);
  }
});

test('hub hooks publish plugin events to the bus (progress/log not lost)', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-hub-events-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      plugins: [{ name: 'hub-event-plugin', package: './hub-event.plugin.cjs', methods: ['hubev.run'] }]
    };`,
  );
  await writeFile(
    path.join(cwd, 'hub-event.plugin.cjs'),
    `module.exports = {
      name: 'hub-event-plugin',
      methods: ['hubev.run'],
      async handle(_method, _params, context) {
        await context.emit({ method: 'event.progress', params: { current: 1 } });
        await context.emit({ method: 'event.log', params: { message: 'hello' } });
        return { result: { ok: true }, events: [] };
      }
    };`,
  );

  const published: Array<{ method: string; params: unknown }> = [];
  const hooks = makeHubHooks((event) =>
    published.push({ method: event.method, params: event.params }),
  );
  const outcome = await hooks.onRequest('hubev.run', { cwd }, 'req-1');

  assert.deepEqual(outcome, { result: { ok: true } });
  assert.deepEqual(
    published.map((e) => e.method),
    ['event.progress', 'event.log'],
    '插件 emit 的事件必须被 publish 到 Hub，否则进度/日志丢失',
  );
});
