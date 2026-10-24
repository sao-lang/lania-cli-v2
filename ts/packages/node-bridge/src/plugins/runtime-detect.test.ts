import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { detectRuntime, isRuntime, type Runtime } from './compiler-shared.js';

async function withFeatureFiles(files: string[], run: (cwd: string) => Promise<void>) {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-runtime-detect-'));
  try {
    for (const file of files) {
      await writeFile(path.join(cwd, file), '', 'utf8');
    }
    await run(cwd);
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
}

test('detectRuntime applies override, config, feature files, then web fallback', async () => {
  const cases: Array<{
    name: string;
    files: string[];
    config: Record<string, unknown>;
    override?: unknown;
    expected: Runtime;
  }> = [
    { name: 'override wins', files: ['go.mod'], config: {}, override: 'rust', expected: 'rust' },
    { name: 'config wins', files: ['package.json'], config: { runtime: 'go' }, expected: 'go' },
    { name: 'go.mod', files: ['go.mod'], config: {}, expected: 'go' },
    { name: 'Cargo.toml', files: ['Cargo.toml'], config: {}, expected: 'rust' },
    { name: 'pyproject.toml', files: ['pyproject.toml'], config: {}, expected: 'python' },
    { name: 'pubspec.yaml', files: ['pubspec.yaml'], config: {}, expected: 'dart' },
    { name: 'package.json', files: ['package.json'], config: {}, expected: 'web' },
    {
      name: 'strong signal beats package.json',
      files: ['go.mod', 'package.json'],
      config: {},
      expected: 'go',
    },
    { name: 'empty directory falls back to web', files: [], config: {}, expected: 'web' },
    {
      name: 'invalid override is ignored',
      files: ['go.mod'],
      config: {},
      override: 'ruby',
      expected: 'go',
    },
  ];

  for (const item of cases) {
    await withFeatureFiles(item.files, async (cwd) => {
      assert.equal(await detectRuntime(item.config, cwd, item.override), item.expected, item.name);
    });
  }
});

test('isRuntime accepts only known runtimes', () => {
  assert.equal(isRuntime('web'), true);
  assert.equal(isRuntime('go'), true);
  assert.equal(isRuntime('ruby'), false);
  assert.equal(isRuntime(undefined), false);
});
