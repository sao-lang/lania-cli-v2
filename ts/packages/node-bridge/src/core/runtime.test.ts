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

import { loadToolConfig } from './runtime.js';

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
