import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  DETECTION_PRIORITY,
  RUNTIME_IDS,
  RUNTIME_TOOLCHAINS,
  choicesFor,
  describeToolchains,
  detectionOrder,
  isRuntimeId,
  lintToolsFor,
  presentationRuntimeIds,
  toolchainFor,
} from './toolchain.js';

// 阶段 4b：detectRuntime 属 compiler-shared（compiler 切片）；测试经动态导入，
// 避免与 compiler 切片形成构建顺序环；切片缺失时相关用例自行跳过。
async function lazyDetectRuntime() {
  // specifier 用变量承载：核与切片互为 devDependency 会造成构建顺序环，
  // 变量形式让 tsc 不在构建期解析切片 dist（缺失时该用例运行期报错即跳过）。
  const specifier = '@lania-cli-v2/bridge-slice-compiler/plugins/compiler-shared.js';
  const mod = (await import(specifier)) as {
    detectRuntime: (options: unknown, cwd: string) => Promise<unknown>;
  };
  return mod.detectRuntime;
}

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

test('deno.json wins over package.json in a dual-runtime project', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-toolchain-'));
  try {
    await writeFile(path.join(cwd, 'deno.json'), '{}', 'utf8');
    await writeFile(path.join(cwd, 'package.json'), '{}', 'utf8');
    let detectRuntimeFn: (options: unknown, cwd: string) => Promise<unknown>;
    try {
      detectRuntimeFn = await lazyDetectRuntime();
    } catch {
      // compiler 切片未随本包安装时（纯 toolchain 环境）跳过该交叉用例。
      t.skip('bridge-slice-compiler is not installed');
      return;
    }
    assert.equal(await detectRuntimeFn({} as never, cwd), 'web');
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

// ---- B14：模板选项（`RuntimeToolchain.presentation`）----

test('every runtime declares template presentation choices', () => {
  for (const id of RUNTIME_IDS) {
    const entry = toolchainFor(id);
    assert.ok(entry, `missing toolchain for ${id}`);
    const presentation = entry.presentation;
    assert.ok(presentation, `${id} has no presentation`);

    // 三个问题分别在 templates 的 questions 里生成，任何一组为空都会让交互表缺项。
    assert.ok(presentation.packageManagers.length > 0, `${id} has no package manager choice`);
    assert.ok(presentation.lintTools.length > 0, `${id} has no lint tool choice`);
    assert.ok(presentation.testTools.length > 0, `${id} has no test tool choice`);

    // 默认值必须属于可选项：否则交互式问答会给出一个选不中的默认值。
    const values = (options: Array<{ value: string }>) => options.map((option) => option.value);
    assert.ok(
      values(presentation.packageManagers).includes(presentation.defaultPackageManager),
      `${id} default package manager is not selectable`,
    );
    assert.ok(
      values(presentation.testTools).includes(presentation.defaultTestTool),
      `${id} default test tool is not selectable`,
    );
    for (const tool of presentation.defaultLintTools) {
      assert.ok(
        values(presentation.lintTools).includes(tool),
        `${id} default lint tool ${tool} is not selectable`,
      );
    }

    // label 是展示文本，缺了会让模板面板出现空选项。
    for (const option of [
      ...presentation.packageManagers,
      ...presentation.lintTools,
      ...presentation.testTools,
    ]) {
      assert.ok(option.value.length > 0, `${id} option without value`);
      assert.ok(option.label.length > 0, `${id} option ${option.value} without label`);
    }
  }
});

test('presentation table covers exactly the seven runtimes', () => {
  // 单一来源在 `@lania-cli-v2/templates/presentation.ts`，这里用运行时 id 反向校验，
  // 防止「新增运行时只改了 toolchain.ts」或「只改了 presentation.ts」。
  assert.deepEqual(presentationRuntimeIds().sort(), [...RUNTIME_IDS].sort());
  for (const id of RUNTIME_IDS) {
    assert.equal(
      choicesFor(id),
      toolchainFor(id)?.presentation,
      `${id} presentation is not shared`,
    );
  }
});

test('presentation is additive: lint/build dispatch data is untouched by it', () => {
  // presentation 允许包含 lint 之外的工具（gofmt / rustfmt 等），
  // 但它**不得**改变分派用的 lintTools —— 否则 lint / build 行为会漂移。
  for (const id of RUNTIME_IDS) {
    const entry = toolchainFor(id);
    assert.ok(entry);
    const dispatchIds = entry.lintTools.map((tool) => tool.id);
    const presentationIds = entry.presentation.lintTools.map((tool) => tool.value);
    for (const toolId of presentationIds) {
      if (entry.lintTools.some((tool) => tool.id === toolId)) {
        assert.ok(dispatchIds.includes(toolId), `${id}: ${toolId} lost its dispatch entry`);
      }
    }
    // 反向：分派层登记的工具可以不出现在模板里（例如 stylelint / textlint），
    // 但一旦出现，value 必须与 adaptor id 完全一致（否则模板选完 lint 什么都不会跑）。
    for (const tool of entry.lintTools) {
      if (presentationIds.includes(tool.id)) {
        const option = entry.presentation.lintTools.find((item) => item.value === tool.id);
        assert.ok(option && option.label.length > 0, `${id}: ${tool.id} label missing`);
      }
    }
  }
});

test('describeToolchains exposes presentation for Rust and doctor', () => {
  const description = describeToolchains();
  const go = description.runtimes.find((runtime) => runtime.id === 'go');
  assert.ok(go, 'missing go runtime');
  assert.deepEqual(go.presentation.packageManagers, [{ value: 'go', label: 'go modules' }]);
  assert.deepEqual(
    go.presentation.lintTools.map((option) => option.value),
    ['golangci-lint', 'gofmt', 'go-vet'],
  );
  // gofmt 只是格式化器：可以出现在 presentation，但**不得**混进分派用的 lintTools。
  assert.ok(!go.lintTools.some((tool) => tool.id === 'gofmt'));
  assert.deepEqual(go.presentation.defaultLintTools, ['gofmt', 'go-vet']);
  assert.equal(go.presentation.defaultTestTool, 'go-test');

  // 逐层浅拷贝：修改返回值不能污染静态表（doctor / Rust 会各自序列化一份）。
  go.presentation.defaultLintTools.push('mutated');
  assert.deepEqual(choicesFor('go').defaultLintTools, ['gofmt', 'go-vet']);
  assert.equal(choicesFor('go').defaultLintTools.includes('mutated'), false);
});
