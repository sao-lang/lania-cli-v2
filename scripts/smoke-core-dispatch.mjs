#!/usr/bin/env node
/**
 * 「只装 core + cmd-env」的调度模式 smoke（文档 §8 阶段 6c）。
 *
 * 场景：只安装 `@lania-cli-v2/core` 与一个命令包（cmd-env），
 * `lan env` 必须能经调度模式路由到 `lan-env[.exe]` 并正常输出；
 * 未安装的命令必须给出安装提示而不是崩溃。
 *
 * 步骤：
 *   1. 构建 `lan-env` 二进制（debug 即可）；
 *   2. 把它放到 cmd-env 的当前平台包里（模拟发布期 staging 产物）；
 *   3. 跑 launcher：`lan env --describe` → 断言输出是 env 的 spec；
 *   4. 跑 launcher：`lan --help` → 断言 env 列为「已安装」；
 *   5. 跑 launcher：`lan <未安装命令>` → 断言退出码 127 且提示安装。
 *
 * 用法：node scripts/smoke-core-dispatch.mjs
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const launcher = path.join(repoRoot, 'packages/distribution/core/npm/core/bin/lan.mjs');
const envBin = process.platform === 'win32' ? 'lan-env.exe' : 'lan-env';
const envPlatformPkg = path.join(
  repoRoot,
  'packages/commands/env/npm',
  `cmd-env-${process.platform === 'win32' ? 'win32-x64' : process.platform === 'darwin' ? 'darwin-arm64' : 'linux-x64'}`,
);

function fail(message) {
  console.error(`[smoke:core-dispatch] ✗ ${message}`);
  process.exit(1);
}

function run(args, options = {}) {
  const result = spawnSync(process.execPath, [launcher, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: { ...process.env, ...(options.env ?? {}) },
  });
  return {
    status: result.status,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  };
}

// 1) 构建 lan-env（无 C++ 链接组件的环境下退化为复用已有产物）
console.log('[smoke:core-dispatch] 准备 lan-env 二进制 …');
const builtBin = path.join(repoRoot, 'target/debug', envBin);
if (!fs.existsSync(builtBin)) {
  // 注意：Windows 上不能用 shell:true —— Git 自带的 `link.exe` 会抢占 MSVC 链接器。
  const build = spawnSync(
    process.platform === 'win32' ? 'cargo.exe' : 'cargo',
    ['build', '--manifest-path', 'Cargo.toml', '-p', 'lania-plugins-command-env', '--bin', 'lan-env'],
    { cwd: repoRoot, encoding: 'utf8' },
  );
  if (build.status !== 0) {
    console.error(build.stdout ?? '');
    console.error(build.stderr ?? '');
    fail('cargo build lan-env 失败');
  }
}
if (!fs.existsSync(builtBin)) fail(`未找到构建产物 ${builtBin}`);

// 2) staging 到当前平台包（模拟发布产物）
const stagedBin = path.join(envPlatformPkg, 'bin', envBin);
fs.mkdirSync(path.dirname(stagedBin), { recursive: true });
fs.copyFileSync(builtBin, stagedBin);

// 3) lan env --describe → env 的 spec
const describe = run(['env', '--describe']);
if (describe.status !== 0) {
  console.error(describe.stdout, describe.stderr);
  fail('lan env --describe 失败');
}
let spec;
try {
  spec = JSON.parse(describe.stdout);
} catch {
  fail(`lan env --describe 输出不是 JSON：${describe.stdout.slice(0, 200)}`);
}
if (spec.name !== 'env') fail(`spec.name 期望 env，实际 ${spec.name}`);

// 4) lan --help → 只列已安装命令（未安装的不出现在命令信息中，阶段 9）
const help = run(['--help']);
if (help.status !== 0) fail('lan --help 失败');
if (!help.stdout.includes('env')) fail('lan --help 未列出 env');
if (help.stdout.includes('npm i @lania-cli-v2/cmd-')) {
  fail('lan --help 不应再直接列出未安装命令的安装提示');
}
if (!help.stdout.includes('lan package list')) {
  fail('lan --help 未指向 lan package list');
}

// 4b) lan package list → 子包清单与安装状态
const pkgList = run(['package', 'list']);
if (pkgList.status !== 0) fail('lan package list 失败');
if (!pkgList.stdout.includes('@lania-cli-v2/cmd-env')) {
  fail('lan package list 未列出 cmd-env');
}
if (!pkgList.stdout.includes('已安装')) fail('lan package list 未标注安装状态');

// 5) 未安装命令 → 127 + 安装提示
const missing = run(['definitely-not-installed']);
if (missing.status !== 127) fail(`未安装命令退出码期望 127，实际 ${missing.status}`);
// Phase 1 未命中 → Phase 2 动态回退（§5.6 F.2）：
// runtime 设施未装时提示安装 runtime；装了但没有该命令时提示未知命令。
const missingText = `${missing.stdout}${missing.stderr}`;
if (!missingText.includes('未知命令') && !missingText.includes('@lania-cli-v2/runtime')) {
  fail('未安装命令缺少「未知命令」或 runtime 安装提示');
}

// 6) lan config packages → 跨包配置视图
const configPackages = run(['config', 'packages']);
if (configPackages.status !== 0) fail('lan config packages 失败');
if (!configPackages.stdout.includes('env')) fail('lan config packages 未列出 env');

// 7) lan config apply → 一键覆盖（无声明键时给出说明而非静默失败）
const configApply = run(['config', 'apply']);
if (configApply.status !== 0) fail('lan config apply 失败');

console.log('[smoke:core-dispatch] ✓ 调度模式 smoke 通过（core + cmd-env + 子包管理面）');
