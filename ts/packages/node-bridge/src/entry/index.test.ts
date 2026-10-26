/**
 * bridge 分发入口的回归测试。
 *
 * 关键点：
 * - 包含文件系统读写/路径解析
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { handleExchange } from './index.js';

test('bridge isolates plugin runtime failures', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-plugin-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = {
      plugins: [
        {
          name: 'demo-plugin',
          package: './plugins/demo.plugin.cjs',
          methods: ['demo.fail']
        }
      ]
    };`,
  );
  await writeProjectFile(
    cwd,
    'plugins/demo.plugin.cjs',
    `module.exports = {
      name: 'demo-plugin',
      methods: ['demo.fail'],
      handle() {
        throw new Error('boom');
      }
    };`,
  );

  const exchange = await handleExchange({
    id: 'req-1',
    method: 'demo.fail',
    params: { cwd },
  });

  assert.equal(exchange.response.error?.code, 'E_PLUGIN_RUNTIME');
  assert.equal(exchange.response.error?.message, 'boom');
});

test('bridge exposes the single-source runtime toolchain table', async () => {
  const exchange = await handleExchange({
    id: 'req-toolchain',
    method: 'toolchain.describe',
    params: {},
  });

  assert.equal(exchange.response.error, undefined);
  const result = exchange.response.result as {
    runtimes: Array<{ id: string; jsCluster: boolean }>;
    detectionOrder: Array<{ file: string; runtime: string }>;
  };
  assert.ok(Array.isArray(result.runtimes));
  assert.deepEqual(result.runtimes.map((entry) => entry.id).sort(), [
    'bun',
    'dart',
    'deno',
    'go',
    'node',
    'python',
    'rust',
  ]);
  // Rust 侧依赖这份顺序做判定，必须存在且非空。
  assert.ok(result.detectionOrder.length > 0);
});

async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}
