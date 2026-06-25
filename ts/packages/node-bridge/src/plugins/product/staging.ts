import { spawnSync } from 'node:child_process';
import { chmod, copyFile, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import fs from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';

import { asRecord, fileExists } from '../../core/runtime.js';

import {
  OFFICIAL_DARWIN_ARM64_PACKAGE_NAME,
  packageRoot,
  repoRoot,
} from './constants.js';
import { copyDirectory, readJsonFile, stageRuntimePackage } from './fs.js';
import type {
  OfficialPlatformPackageDescriptor,
  ProductBundlePlatformSource,
  ProductBundlePlatformTarball,
  ProductBundlePlatformMatrixEntry,
  ProductPublishBundle,
  ResolvedPlatformBinarySource,
} from './types.js';
import { firstNonEmptyString, uniqueStrings } from './utils.js';

// 这一组 helper 负责 pack/publish 前的“产物暂存”阶段：
// - 把 node-bridge runtime 整理进 install-root / npm package
// - 生成 wrapper 脚本，约定好运行时需要的环境变量
// - 在发布规划场景下，把官方 CLI 及平台二进制一起打包成 bundle

export async function stageNodeBridgePayload(targetDir: string): Promise<void> {
  await rm(targetDir, { recursive: true, force: true });
  await mkdir(targetDir, { recursive: true });

  // 先复制 dist 和 package.json，再把运行时依赖递归暂存进 node_modules。
  await copyFile(join(packageRoot, 'package.json'), join(targetDir, 'package.json'));
  await copyDirectory(join(packageRoot, 'dist'), join(targetDir, 'dist'));
  const nodeModulesTarget = join(targetDir, 'node_modules');
  await mkdir(nodeModulesTarget, { recursive: true });

  const nodeBridgePackage = await readJsonFile(join(packageRoot, 'package.json'));
  const runtimeDependencies = [
    ...Object.keys(asRecord(nodeBridgePackage.dependencies)),
    ...Object.keys(asRecord(nodeBridgePackage.optionalDependencies)),
  ];
  const stagedPackages = new Set<string>();
  for (const packageName of runtimeDependencies) {
    await stageRuntimePackage(packageRoot, packageName, nodeModulesTarget, stagedPackages);
  }
}

export function runNpmPack(outputRoot: string): { filename: string } {
  // `npm pack --json` 返回的是“已生成 tarball 列表”，这里统一只取第一项。
  const result = spawnSync('npm', ['pack', '--json', '--ignore-scripts'], {
    cwd: outputRoot,
    encoding: 'utf8',
    env: process.env,
  });
  if (result.status !== 0) {
    throw new Error(
      `npm pack failed in ${outputRoot}: ${result.stderr || result.stdout || 'unknown error'}`,
    );
  }
  const parsed = JSON.parse(result.stdout.trim()) as Array<Record<string, unknown>>;
  const first = parsed[0];
  const filename = typeof first?.filename === 'string' ? first.filename : null;
  if (!filename) {
    throw new Error(`npm pack did not return a tarball filename for ${outputRoot}`);
  }
  return { filename };
}

export async function stageOfficialCliBundle(
  outputRoot: string,
  params: Record<string, unknown>,
  platformBinarySource: ResolvedPlatformBinarySource | null,
): Promise<ProductPublishBundle> {
  // 生成“官方发布 bundle”：
  // - 一份通用 CLI 包（JS wrapper + node-bridge）
  // - 零到多份平台二进制包
  //
  // 返回值不只是 tarball 路径，还会保留 platformMatrix，方便发布规划阶段解释
  // 哪个平台已经就绪、哪个平台缺 package、哪个平台缺 binary。
  const bundleRoot = join(outputRoot, 'bundle');
  await rm(bundleRoot, { recursive: true, force: true });
  await mkdir(bundleRoot, { recursive: true });

  // 先暂存“官方 CLI 主包”，它本身不携带平台二进制，只负责 wrapper 和 node-bridge runtime。
  const cliPackageRoot = join(bundleRoot, 'cli-package');
  await mkdir(join(cliPackageRoot, 'bin'), { recursive: true });
  await copyFile(join(repoRoot, 'npm/cli/bin/lan.mjs'), join(cliPackageRoot, 'bin', 'lan.mjs'));
  await copyFile(join(repoRoot, 'npm/cli/package.json'), join(cliPackageRoot, 'package.json'));
  await stageNodeBridgePayload(join(cliPackageRoot, 'lib', 'node-bridge'));
  const cliTarball = runNpmPack(cliPackageRoot);

  const platformTarballs: ProductBundlePlatformTarball[] = [];
  const platformMatrix: ProductBundlePlatformMatrixEntry[] = [];
  const descriptors = await discoverOfficialPlatformPackages();

  // 平台包是可选矩阵项；即使缺少某一项，也要把缺失原因记录进规划结果，而不是直接失败。
  for (const descriptor of descriptors) {
    if (!descriptor.packageExists) {
      platformMatrix.push({
        packageName: descriptor.packageName,
        platform: descriptor.platform,
        version: descriptor.version,
        status: 'package_missing',
        tarball: null,
        source: 'optional_dependency',
      });
      continue;
    }

    const resolvedBinary = await resolveBinarySourceForDescriptor(
      descriptor,
      params,
      platformBinarySource,
    );
    if (!resolvedBinary) {
      platformMatrix.push({
        packageName: descriptor.packageName,
        platform: descriptor.platform,
        version: descriptor.version,
        status: 'binary_missing',
        tarball: null,
        source: 'package_dir',
      });
      continue;
    }

    // 先组装一个临时 package 目录，再交给 `npm pack`，这样 tarball 里的结构和元数据更稳定。
    const stagedDirName = `${basename(descriptor.packageRoot)}-package`;
    const platformPackageRoot = join(bundleRoot, stagedDirName);
    await mkdir(join(platformPackageRoot, 'bin'), { recursive: true });
    await copyFile(
      join(descriptor.packageRoot, 'package.json'),
      join(platformPackageRoot, 'package.json'),
    );
    await copyFile(resolvedBinary.binaryPath, join(platformPackageRoot, 'bin', 'lania-cli'));
    await chmod(join(platformPackageRoot, 'bin', 'lania-cli'), 0o755);

    const platformTarball = runNpmPack(platformPackageRoot);
    const tarballPath = `./bundle/${stagedDirName}/${platformTarball.filename}`;
    platformTarballs.push({
      packageName: descriptor.packageName,
      platform: descriptor.platform,
      version: descriptor.version,
      tarball: tarballPath,
      source: resolvedBinary.source,
    });
    platformMatrix.push({
      packageName: descriptor.packageName,
      platform: descriptor.platform,
      version: descriptor.version,
      status: 'ready',
      tarball: tarballPath,
      source: resolvedBinary.source,
    });
  }

  return {
    root: './bundle',
    cliTarball: `./bundle/cli-package/${cliTarball.filename}`,
    platformTarball: platformTarballs[0]?.tarball ?? null,
    platformTarballs,
    platformMatrix,
  };
}

export async function stageProductWrapper(outputRoot: string, binaryName: string): Promise<string> {
  // install-root 模式下的 wrapper：
  // 它不会自己实现 CLI，而是重新拉起宿主二进制（默认 `lan`），同时把 node-bridge 和
  // product 根目录通过环境变量注入进去。
  const wrapperPath = join(outputRoot, 'bin', binaryName);
  await mkdir(dirname(wrapperPath), { recursive: true });
  await writeFile(
    wrapperPath,
    `#!/usr/bin/env node
const { spawn } = require('node:child_process');
const path = require('node:path');
const process = require('node:process');

const here = __dirname;
const installRoot = path.resolve(here, '..');
const bridgeDir = path.join(installRoot, 'lib', 'node-bridge');
const productRoot = path.join(installRoot, 'lib', 'product');
const binary = process.env.LANIA_PRODUCT_HOST_BINARY ?? process.env.LANIA_HOST_BINARY ?? 'lan';

const child = spawn(binary, process.argv.slice(2), {
  stdio: 'inherit',
  env: {
    ...process.env,
    LANIA_NODE_BRIDGE_DIR: process.env.LANIA_NODE_BRIDGE_DIR ?? bridgeDir,
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? productRoot,
    LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
  },
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
`,
    'utf8',
  );
  await chmod(wrapperPath, 0o755);
  return wrapperPath;
}

export async function stagePublishedWrapper(outputRoot: string, binaryName: string): Promise<string> {
  // npm 发布包模式下的 wrapper：
  // 它需要先从可选依赖里解析出当前平台对应的二进制包，再转而执行那个真实二进制。
  // 这层 wrapper 的价值在于把“平台选择 + 环境变量注入”从业务 CLI 本体里剥离出来。
  const wrapperPath = join(outputRoot, 'bin', `${binaryName}.mjs`);
  await mkdir(dirname(wrapperPath), { recursive: true });
  await writeFile(
    wrapperPath,
    `#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);

function resolvePlatformBinaryPackage() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === 'darwin' && arch === 'arm64') {
    return '@lania-cli/cli-darwin-arm64';
  }
  if (platform === 'linux' && arch === 'x64') {
    return '@lania-cli/cli-linux-x64';
  }
  return null;
}

function resolveBinaryPath(pkgName) {
  const cliPkgJson = require.resolve('@lania-cli/cli/package.json');
  const cliRequire = createRequire(cliPkgJson);
  const pkgJson = cliRequire.resolve(\`\${pkgName}/package.json\`);
  return path.join(path.dirname(pkgJson), 'bin', 'lania-cli');
}

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');
const bridgeDir = path.join(packageRoot, 'lib', 'node-bridge');
const productRoot = path.join(packageRoot, 'lib', 'product');
const binPkg = resolvePlatformBinaryPackage();

if (!binPkg) {
  console.error(\`Unsupported platform: \${process.platform} \${process.arch}. Supported packaged platforms: darwin arm64, linux x64.\`);
  process.exit(1);
}

let binaryPath;
try {
  binaryPath = resolveBinaryPath(binPkg);
} catch {
  console.error(\`Missing optional binary dependency \${binPkg}. Reinstall the package, or ensure optionalDependencies are enabled.\`);
  process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  env: {
    ...process.env,
    LANIA_NODE_BRIDGE_DIR: process.env.LANIA_NODE_BRIDGE_DIR ?? bridgeDir,
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? productRoot,
    LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
  },
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
`,
    'utf8',
  );
  await chmod(wrapperPath, 0o755);
  return wrapperPath;
}

export async function resolvePlatformBinarySource(
  params: Record<string, unknown>,
): Promise<ResolvedPlatformBinarySource | null> {
  // 为“当前宿主平台”挑选一个可用的二进制来源。
  // 候选按优先级依次来自：请求参数 -> 官方 staging 默认值 -> 环境变量。
  const descriptors = await discoverOfficialPlatformPackages();
  const hostPlatform = `${process.platform}-${process.arch}`;
  const defaultDescriptor =
    descriptors.find((entry) => entry.platform === hostPlatform) ??
    descriptors.find((entry) => entry.packageName === OFFICIAL_DARWIN_ARM64_PACKAGE_NAME) ??
    null;
  if (!defaultDescriptor) {
    return null;
  }

  // 候选按顺序检查，命中的第一个存在路径即视为最终来源。
  const candidates: Array<{
    path: string;
    source: ResolvedPlatformBinarySource['source'];
  }> = [];
  if (typeof params.hostBinaryPath === 'string' && params.hostBinaryPath.trim().length > 0) {
    candidates.push({
      path: params.hostBinaryPath.trim(),
      source: 'request',
    });
  }
  candidates.push({
    path: defaultDescriptor.binaryPath,
    source: 'official_staging',
  });
  const envCandidates = [
    process.env.LANIA_PRODUCT_HOST_BINARY_SOURCE,
    process.env.LANIA_PRODUCT_HOST_BINARY,
    process.env.LANIA_HOST_BINARY,
  ];
  for (const candidate of envCandidates) {
    if (typeof candidate === 'string' && candidate.trim().length > 0) {
      candidates.push({
        path: candidate.trim(),
        source: 'environment',
      });
    }
  }

  for (const candidate of candidates) {
    if (!(await fileExists(candidate.path))) {
      continue;
    }
    return {
      packageName: defaultDescriptor.packageName,
      packageRoot: defaultDescriptor.packageRoot,
      platform: defaultDescriptor.platform,
      binaryPath: candidate.path,
      source: candidate.source,
    };
  }

  return null;
}

async function discoverOfficialPlatformPackages(): Promise<OfficialPlatformPackageDescriptor[]> {
  // 根据 npm/cli/package.json 的 optionalDependencies 反推“官方支持的平台矩阵”。
  const cliPackage = await readJsonFile(join(repoRoot, 'npm/cli/package.json'));
  const optionalDependencies = asRecord(cliPackage.optionalDependencies);
  const result: OfficialPlatformPackageDescriptor[] = [];

  for (const [packageName, versionValue] of Object.entries(optionalDependencies)) {
    if (!packageName.startsWith('@lania-cli/cli-')) {
      continue;
    }
    const packageDirName = packageName.replace('@lania-cli/', '');
    const packageRootDir = join(repoRoot, 'npm', packageDirName);
    const packageExists = await fileExists(join(packageRootDir, 'package.json'));
    const packageJson = packageExists ? await readJsonFile(join(packageRootDir, 'package.json')) : {};
    const osValue = Array.isArray(packageJson.os) ? packageJson.os[0] : null;
    const cpuValue = Array.isArray(packageJson.cpu) ? packageJson.cpu[0] : null;

    // `platform` 既用于匹配，也用于环境变量后缀和发布规划展示。
    const platform =
      typeof osValue === 'string' && typeof cpuValue === 'string'
        ? `${osValue}-${cpuValue}`
        : packageDirName.replace(/^cli-/, '');

    result.push({
      packageName,
      packageRoot: packageRootDir,
      platform,
      version: String(packageJson.version ?? versionValue ?? '0.1.0'),
      binaryPath: join(packageRootDir, 'bin', 'lania-cli'),
      packageExists,
    });
  }

  return result.sort((left, right) => left.packageName.localeCompare(right.packageName));
}

async function resolveBinarySourceForDescriptor(
  descriptor: OfficialPlatformPackageDescriptor,
  params: Record<string, unknown>,
  genericSource: ResolvedPlatformBinarySource | null,
): Promise<{ binaryPath: string; source: ProductBundlePlatformSource } | null> {
  // 为某个具体平台包挑选二进制来源。
  // 这里允许“通用默认来源”和“按平台覆盖来源”并存，便于发布时局部覆盖某个平台的二进制。
  const candidates: Array<{ path: string; source: ProductBundlePlatformSource }> = [];

  if (genericSource && genericSource.packageName === descriptor.packageName) {
    candidates.push({
      path: genericSource.binaryPath,
      source: genericSource.source,
    });
  }

  // 允许传入 JSON map，例如：
  // {"darwin-arm64":"/path/to/bin","@lania-cli/cli-darwin-arm64":"/path/to/bin"}
  const platformBinaryPaths = normalizePlatformBinaryPaths(params.platformBinaryPaths);
  const requestCandidates = [
    platformBinaryPaths[descriptor.platform],
    platformBinaryPaths[descriptor.packageName],
  ];
  for (const candidate of requestCandidates) {
    if (typeof candidate === 'string' && candidate.trim().length > 0) {
      candidates.push({
        path: candidate.trim(),
        source: 'request',
      });
    }
  }

  for (const candidate of resolvePlatformBinaryDirCandidates(params, descriptor, 'request')) {
    candidates.push(candidate);
  }

  for (const candidate of resolvePlatformBinaryEnvCandidates(descriptor.platform)) {
    candidates.push({
      path: candidate,
      source: 'environment',
    });
  }

  for (const candidate of resolvePlatformBinaryDirCandidatesFromEnv(descriptor)) {
    candidates.push(candidate);
  }

  // 官方默认兜底：直接使用平台包目录里的 bin/lania-cli。
  candidates.push({
    path: descriptor.binaryPath,
    source: 'official_staging',
  });

  for (const candidate of candidates) {
    if (!(await fileExists(candidate.path))) {
      continue;
    }
    return {
      binaryPath: candidate.path,
      source: candidate.source,
    };
  }

  return null;
}

function resolvePlatformBinaryEnvCandidates(platform: string): string[] {
  const suffix = platformToEnvSuffix(platform);
  const values = [
    process.env[`LANIA_PRODUCT_BINARY_SOURCE_${suffix}`],
    process.env[`LANIA_PRODUCT_${suffix}_BINARY_SOURCE`],
    process.env[`LANIA_CLI_${suffix}_BINARY_SOURCE`],
  ];
  return values
    .filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
    .map((value) => value.trim());
}

function resolvePlatformBinaryDirCandidates(
  params: Record<string, unknown>,
  descriptor: OfficialPlatformPackageDescriptor,
  source: ProductBundlePlatformSource,
): Array<{ path: string; source: ProductBundlePlatformSource }> {
  const raw = firstNonEmptyString(params.platformBinariesDir);
  if (!raw) {
    return [];
  }
  return buildPlatformBinaryDirCandidates(raw, descriptor, source);
}

function resolvePlatformBinaryDirCandidatesFromEnv(
  descriptor: OfficialPlatformPackageDescriptor,
): Array<{ path: string; source: ProductBundlePlatformSource }> {
  const values = [
    process.env.LANIA_PRODUCT_BINARY_SOURCES_DIR,
    process.env.LANIA_PRODUCT_PLATFORM_BINARIES_DIR,
    process.env.LANIA_CLI_PLATFORM_BINARIES_DIR,
  ]
    .filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
    .map((value) => value.trim());

  return values.flatMap((value) => buildPlatformBinaryDirCandidates(value, descriptor, 'environment'));
}

function buildPlatformBinaryDirCandidates(
  rootDir: string,
  descriptor: OfficialPlatformPackageDescriptor,
  source: ProductBundlePlatformSource,
): Array<{ path: string; source: ProductBundlePlatformSource }> {
  const candidates = uniqueStrings([
    join(rootDir, descriptor.platform, 'lania-cli'),
    join(rootDir, descriptor.platform, 'bin', 'lania-cli'),
    join(rootDir, basename(descriptor.packageRoot), 'lania-cli'),
    join(rootDir, basename(descriptor.packageRoot), 'bin', 'lania-cli'),
    join(rootDir, descriptor.packageName, 'lania-cli'),
    join(rootDir, descriptor.packageName, 'bin', 'lania-cli'),
  ]);
  return candidates.map((path) => ({ path, source }));
}

function platformToEnvSuffix(platform: string): string {
  return platform.replace(/[^A-Za-z0-9]+/g, '_').toUpperCase();
}

function normalizePlatformBinaryPaths(input: unknown): Record<string, unknown> {
  if (typeof input === 'string') {
    const trimmed = input.trim();
    if (!trimmed) {
      return {};
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(trimmed);
    } catch (error) {
      throw new Error(
        `product.publish received invalid platformBinaryPaths JSON: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error('product.publish requires platformBinaryPaths to be a JSON object');
    }
    return parsed as Record<string, unknown>;
  }
  return asRecord(input);
}
