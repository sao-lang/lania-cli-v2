/**
 * commitizen 与 commitlint 插件的回归测试。
 *
 * 关键点：
 * - 包含文件系统读写/路径解析
 * - 包含 JSON 协议/序列化
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { commitizenPlugin } from './commitizen.js';
import { commitlintPlugin } from './commitlint.js';

test('commitizen loads .czrc.cjs and applies configured types and subject limit', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-commitizen-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    '.czrc.cjs',
    `module.exports = {
      types: [{ value: 'feat', name: 'feat: feature' }],
      subjectLimit: 12
    };`,
  );

  const handled = await commitizenPlugin.handle(
    'commitizen.run',
    {
      kind: 'chore',
      scope: 'sync',
      subject: 'long subject text',
    },
    { cwd },
  );

  assert.ok(handled);
  assert.equal(handled.result.message, 'feat(sync): long subject');
  assert.equal(handled.result.kind, 'feat');
  assert.equal(handled.result.subject, 'long subject');
  assert.equal(handled.result.configLoaded, true);
  assert.ok(String(handled.result.configPath).endsWith('.czrc.cjs'));
  assert.ok(
    handled.events.some((event) =>
      event.method === 'event.log' &&
      String(event.params?.message ?? '').includes('adjusted to "feat"'),
    ),
  );
});

test('commitizen resolves custom config path from package.json commitizen metadata', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-commitizen-pkg-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify(
      {
        name: 'demo-app',
        config: {
          commitizen: {
            path: './node_modules/cz-customizable',
          },
          'cz-customizable': {
            config: './config/custom-cz.cjs',
          },
        },
      },
      null,
      2,
    ),
  );
  await writeProjectFile(
    cwd,
    'config/custom-cz.cjs',
    `module.exports = {
      types: [{ value: 'fix', name: 'fix: bug fix' }]
    };`,
  );

  const handled = await commitizenPlugin.handle(
    'commitizen.run',
    {
      kind: 'feat',
      subject: 'repair sync flow',
    },
    { cwd },
  );

  assert.ok(handled);
  assert.equal(handled.result.message, 'fix: repair sync flow');
  assert.ok(String(handled.result.configPath).endsWith('config/custom-cz.cjs'));
});

test('commitlint loads commitlint.config.cjs and applies configured rules', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-commitlint-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'commitlint.config.cjs',
    `module.exports = {
      rules: {
        'type-enum': [2, 'always', ['feat', 'fix']],
        'scope-empty': [2, 'never'],
        'header-max-length': [2, 'always', 20]
      }
    };`,
  );

  const invalid = await commitlintPlugin.handle(
    'commitlint.run',
    { message: 'chore: this subject is too long' },
    { cwd },
  );
  const valid = await commitlintPlugin.handle(
    'commitlint.run',
    { message: 'feat(sync): short' },
    { cwd },
  );

  assert.ok(invalid);
  assert.equal(invalid.result.valid, false);
  assert.deepEqual(invalid.result.errors, [
    'type must be one of: feat, fix',
    'scope must not be empty',
    'header must not be longer than 20 characters',
  ]);
  assert.ok(valid);
  assert.equal(valid.result.valid, true);
  assert.equal(valid.result.configLoaded, true);
  assert.ok(String(valid.result.configPath).endsWith('commitlint.config.cjs'));
});

async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}
