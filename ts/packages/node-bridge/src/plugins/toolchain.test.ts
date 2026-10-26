import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { detectRuntime } from './compiler-shared.js';
import {
  DETECTION_PRIORITY,
  RUNTIME_IDS,
  RUNTIME_TOOLCHAINS,
  describeToolchains,
  detectionOrder,
  isRuntimeId,
  lintToolsFor,
  toolchainFor,
} from './toolchain.js';

test('every runtime id has exactly one toolchain entry', () => {
  assert.equal(RUNTIME_TOOLCHAINS.length, RUNTIME_IDS.length);
  for (const id of RUNTIME_IDS) {
    assert.ok(toolchainFor(id), `missing toolchain for ${id}`);
  }
});

test('only node/bun/deno are marked as the JS cluster', () => {
  const jsCluster = RUNTIME_TOOLCHAINS.filter((entry) => entry.jsCluster).map((entry) => entry.id);
  assert.deepEqual(jsCluster.sort(), ['bun', 'deno', 'node']);
});

test('node has no default build/dev command (it uses vite/webpack adapters)', () => {
  const node = toolchainFor('node');
  assert.ok(node);
  assert.deepEqual(node.buildCommand, []);
  assert.deepEqual(node.devCommand, []);
});

test('non-JS runtimes all declare a build command', () => {
  for (const id of ['python', 'go', 'rust', 'dart'] as const) {
    const entry = toolchainFor(id);
    assert.ok(entry, `missing ${id}`);
    assert.ok(entry.buildCommand.length > 0, `${id} has no build command`);
  }
});

test('deno lint/fmt are marked as not-in-process (builtin Rust implementation)', () => {
  const tools = lintToolsFor('deno');
  assert.ok(tools.length > 0);
  for (const tool of tools) {
    assert.equal(tool.inProcess, false, `${tool.id} must be spawned`);
    assert.equal(tool.runner, 'deno');
  }
});

test('bun lint tools go through bunx rather than node_modules/.bin', () => {
  for (const tool of lintToolsFor('bun')) {
    assert.equal(tool.runner, 'bunx', `${tool.id} should use bunx`);
  }
});

test('detection priority keeps bun.lock before package.json', () => {
  const files = DETECTION_PRIORITY.map((item) => item.file);
  assert.ok(files.indexOf('bun.lockb') < files.indexOf('package.json'));
  assert.ok(files.indexOf('bun.lock') < files.indexOf('package.json'));
});

test('detection priority keeps strong non-JS signals before package.json', () => {
  const files = DETECTION_PRIORITY.map((item) => item.file);
  const packageJson = files.indexOf('package.json');
  for (const file of ['go.mod', 'Cargo.toml', 'pyproject.toml', 'pubspec.yaml']) {
    assert.ok(files.indexOf(file) < packageJson, `${file} must precede package.json`);
  }
});

test('deno.json wins over package.json in a dual-runtime project', async () => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-toolchain-'));
  try {
    await writeFile(path.join(cwd, 'deno.json'), '{}', 'utf8');
    await writeFile(path.join(cwd, 'package.json'), '{}', 'utf8');
    assert.equal(await detectRuntime({}, cwd), 'web');
    // `web` 是 compiler 侧对 JS 簇的表示；这里真正要确认的是「不是被当作 node 兜底」。
    // 用 toolchain 层的判定再确认一次具体是 deno。
    const firstMatch = detectionOrder().find((item) =>
      ['deno.json', 'package.json'].includes(item.file),
    );
    assert.equal(firstMatch?.runtime, 'deno');
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('bun.lockb wins over package.json', async () => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-toolchain-bun-'));
  try {
    await writeFile(path.join(cwd, 'bun.lockb'), '', 'utf8');
    await writeFile(path.join(cwd, 'package.json'), '{}', 'utf8');
    const firstMatch = detectionOrder().find((item) =>
      ['bun.lockb', 'package.json'].includes(item.file),
    );
    assert.equal(firstMatch?.runtime, 'bun');
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('isRuntimeId accepts known runtimes only', () => {
  for (const id of RUNTIME_IDS) {
    assert.equal(isRuntimeId(id), true);
  }
  assert.equal(isRuntimeId('web'), false);
  assert.equal(isRuntimeId('cobol'), false);
  assert.equal(isRuntimeId(undefined), false);
});

test('describeToolchains is serialisable and carries detection order', () => {
  const description = describeToolchains();
  assert.equal(description.runtimes.length, RUNTIME_IDS.length);
  assert.ok(description.detectionOrder.length > 0);
  // 必须能无损跨进程序列化（Rust 侧读取的就是它）。
  const roundTripped = JSON.parse(JSON.stringify(description));
  assert.deepEqual(roundTripped, description);
  for (const runtime of roundTripped.runtimes) {
    assert.equal(typeof runtime.id, 'string');
    assert.equal(typeof runtime.jsCluster, 'boolean');
    assert.ok(Array.isArray(runtime.lintTools));
  }
});

test('toolchainFor returns null for unknown ids', () => {
  assert.equal(toolchainFor('cobol'), null);
});
