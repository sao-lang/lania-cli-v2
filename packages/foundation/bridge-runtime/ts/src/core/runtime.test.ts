/**
 * 配置加载与模块解析逻辑的回归测试。
 *
 * 关键点：
 * - 包含文件系统读写/路径解析
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  LAN_CONFIG_SEARCH_PLACES,
  loadLanConfig,
  loadToolConfig,
  resolveConfigContext,
} from './runtime.js';

test('lan.config candidate list stays the single 7-element authority', () => {
  // Rust 侧不再维护副本，所以这份列表就是唯一契约（§6.5.3）。
  assert.deepEqual(LAN_CONFIG_SEARCH_PLACES, [
    'lan.config.js',
    'lan.config.cjs',
    'lan.config.mjs',
    'lan.config.json',
    'lan.config.ts',
    'lan.config.mts',
    'lan.config.cts',
  ]);
});

test('loadLanConfig finds .mts and .cts variants', async (t) => {
  for (const file of ['lan.config.mts', 'lan.config.cts', 'lan.config.mjs']) {
    const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-dev-'));
    t.after(async () => rm(cwd, { recursive: true, force: true }));
    await writeProjectFile(cwd, file, 'export default { buildTool: "vite" };\n');
    const loaded = await loadLanConfig(cwd);
    assert.equal(loaded.exists, true, `${file} should be found`);
    assert.ok(loaded.configPath?.endsWith(file), `${loaded.configPath} should end with ${file}`);
  }
});

test('loadLanConfig evaluates function configs with { env, cwd, command }', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-fn-config-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  await writeProjectFile(
    cwd,
    'lan.config.ts',
    `export default ({ env, cwd, command }: { env: string; cwd: string; command: string }) => ({
      buildTool: env === 'prod' ? 'webpack' : 'vite',
      custom: { env, cwd, command },
    });\n`,
  );

  const previous = process.env.LANIA_ENV;
  process.env.LANIA_ENV = 'prod';
  t.after(() => {
    if (previous === undefined) delete process.env.LANIA_ENV;
    else process.env.LANIA_ENV = previous;
  });

  const loaded = await loadLanConfig(cwd, { command: 'build' });
  assert.equal(loaded.error, undefined);
  assert.equal(loaded.config.buildTool, 'webpack');
  const custom = loaded.config.custom as Record<string, unknown>;
  assert.equal(custom.env, 'prod');
  assert.equal(custom.cwd, cwd.replace(/\\/g, path.sep));
  assert.equal(custom.command, 'build');
});

test('loadLanConfig defaults env to development and ignores NODE_ENV', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-env-default-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  await writeProjectFile(
    cwd,
    'lan.config.ts',
    `export default ({ env }: { env: string }) => ({ custom: { env } });\n`,
  );

  const previousLania = process.env.LANIA_ENV;
  const previousNode = process.env.NODE_ENV;
  delete process.env.LANIA_ENV;
  process.env.NODE_ENV = 'production';
  t.after(() => {
    if (previousLania === undefined) delete process.env.LANIA_ENV;
    else process.env.LANIA_ENV = previousLania;
    if (previousNode === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = previousNode;
  });

  const loaded = await loadLanConfig(cwd);
  assert.equal((loaded.config.custom as Record<string, unknown>).env, 'development');
});

test('loadLanConfig surfaces load failures instead of silently returning {}', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-bad-config-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  // 语法错误：`import` 一个不存在的模块会让动态 import 直接抛错。
  await writeProjectFile(
    cwd,
    'lan.config.ts',
    `import missing from './definitely-not-here.js';\nexport default { missing };\n`,
  );

  const loaded = await loadLanConfig(cwd);
  assert.equal(loaded.exists, true);
  assert.deepEqual(loaded.config, {});
  assert.ok(loaded.error && loaded.error.length > 0, 'error must be reported');
});

test('resolveConfigContext defaults env to development', () => {
  const previous = process.env.LANIA_ENV;
  delete process.env.LANIA_ENV;
  try {
    const context = resolveConfigContext('/repo', 'lint');
    assert.equal(context.env, 'development');
    assert.equal(context.cwd, '/repo');
    assert.equal(context.command, 'lint');
  } finally {
    if (previous !== undefined) process.env.LANIA_ENV = previous;
  }
});

test('loadToolConfig resolves stylelint extended search places', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-stylelint-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(cwd, '.stylelintrc.yaml', 'rules:\n  color-no-invalid-hex: true\n');

  const loaded = await loadToolConfig(cwd, 'stylelint');

  assert.equal(loaded.exists, true);
  assert.ok(loaded.configPath?.endsWith('.stylelintrc.yaml'));
});

test('loadToolConfig resolves oxlint config search places', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-oxlint-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(cwd, '.oxlintrc.json', '{ "rules": {} }\n');

  const loaded = await loadToolConfig(cwd, 'oxlint');

  assert.equal(loaded.exists, true);
  assert.ok(loaded.configPath?.endsWith('.oxlintrc.json'));
});

test('loadToolConfig resolves oxfmt config search places', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-oxfmt-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(cwd, '.oxfmtrc.json', '{ "semi": true }\n');

  const loaded = await loadToolConfig(cwd, 'oxfmt');

  assert.equal(loaded.exists, true);
  assert.ok(loaded.configPath?.endsWith('.oxfmtrc.json'));
});

test('loadToolConfig resolves textlint extended search places', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-textlint-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(cwd, '.textlintrc.yml', 'rules:\n  preset-ja-spacing: true\n');

  const loaded = await loadToolConfig(cwd, 'textlint');

  assert.equal(loaded.exists, true);
  assert.ok(loaded.configPath?.endsWith('.textlintrc.yml'));
});

async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}
