import fs from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

// NOTE:
// This module centralizes "where am I" and other constants shared by the product plugin.
// Keeping these in one file avoids subtle drift when different handlers compute roots differently.

export const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
// `packages/foundation/bridge-slices/product/ts` -> repository root (`lania-cli-v2`)
export const repoRoot = resolve(packageRoot, '..', '..', '..', '..', '..');

// node-bridge payload 的根目录（用于把 bridge 运行时打包进产品产物）：
// - 总包/发布形态下，product 切片位于 payload 的 dist/plugins/product 内，
//   packageRoot 即 payload 根（dist/entry/stdio.js 存在）；
// - 开发/测试形态下，product 切片作为独立包运行，此时回落到
//   `@lania-cli-v2/bridge-runtime` 的安装目录。
const require = createRequire(import.meta.url);

export function resolveBridgePayloadRoot(): string {
  if (fs.existsSync(join(packageRoot, 'dist', 'entry', 'stdio.js'))) {
    return packageRoot;
  }
  try {
    return dirname(require.resolve('@lania-cli-v2/bridge-runtime/package.json'));
  } catch {
    return packageRoot;
  }
}

export const bridgePayloadRoot = resolveBridgePayloadRoot();

// Report schema versions (bumped when we make breaking changes to persisted JSON outputs).
export const PRODUCT_REPORT_VERSION = 1;
export const COMPAT_REPORT_VERSION = 1;

// For publish flows we need a stable default platform package to fall back to (useful in dev).
export const OFFICIAL_DARWIN_ARM64_PACKAGE_NAME = '@lania-cli-v2/cli-darwin-arm64';

/**
 * Windows 上 `spawnSync('npm')` 不带 shell 时解析不到 `npm.cmd`（ENOENT），
 * 这里统一把裸 `npm` 解析成平台对应的可执行名；显式配置的 `npmBin` 或
 * 非 npm 程序原样返回。
 */
export function resolveNpmBin(requested?: string | null): string {
  const program = requested ?? 'npm';
  if (process.platform !== 'win32') {
    return program;
  }
  return program === 'npm' ? 'npm.cmd' : program;
}

/**
 * Windows 解析 `npm.cmd` 时必须带 shell（Node ≥20.12 对 .cmd/.bat 强制此约束，
 * 否则 spawnSync 报 EINVAL）。npm 的子命令与参数均为受控字面量，无注入面。
 */
export function resolveNpmSpawnShell(): boolean {
  return process.platform === 'win32';
}
