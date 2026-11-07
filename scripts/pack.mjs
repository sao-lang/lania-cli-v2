import { spawnSync } from 'node:child_process';
import console from 'node:console';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { fetchPlatformBinaries } from './fetch-platform-binaries.mjs';
import { discoverPackages } from './lib/discover-packages.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, ...(options.env ?? {}) },
  });
  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(' ')}`);
  }
}

function canRun(command, args = ['--version']) {
  const result = spawnSync(command, args, {
    stdio: 'ignore',
    cwd: repoRoot,
    env: process.env,
  });
  return result.status === 0;
}

function runPnpm(pnpmArgs, options = {}) {
  if (canRun('pnpm')) {
    run('pnpm', pnpmArgs, options);
    return;
  }
  // Fallback: use npm to execute a pinned pnpm version.
  // This avoids corepack signature issues in some environments.
  const workspacePkg = readJson(path.join(repoRoot, 'ts/package.json'));
  const pinned =
    (workspacePkg.packageManager ?? 'pnpm@10.25.0').split('@').slice(1).join('@') || '10.25.0';
  run('npm', ['exec', '--yes', `pnpm@${pinned}`, '--', ...pnpmArgs], options);
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function rmIfExists(target) {
  if (fs.existsSync(target)) {
    fs.rmSync(target, { recursive: true, force: true });
  }
}

function copyDir(src, dst) {
  fs.cpSync(src, dst, { recursive: true });
}

function copyFile(src, dst, mode) {
  ensureDir(path.dirname(dst));
  fs.copyFileSync(src, dst);
  if (mode) {
    fs.chmodSync(dst, mode);
  }
}


function readRustWorkspaceVersion() {
  const cargoToml = fs.readFileSync(path.join(repoRoot, 'rust/Cargo.toml'), 'utf8');
  const marker = '[workspace.package]';
  const idx = cargoToml.indexOf(marker);
  if (idx === -1) {
    throw new Error('rust/Cargo.toml missing [workspace.package]');
  }
  const tail = cargoToml.slice(idx);
  const match = tail.match(/\nversion\s*=\s*"([^"]+)"/);
  if (!match) {
    throw new Error('rust/Cargo.toml missing workspace.package version');
  }
  return match[1];
}

/**
 * 版本族校验（文档 §6.2 / §6.5）：按 `versionFamily` 分组，
 * 组内 manifest / npm package.json / 平台包 / describes 全等；**组间允许不同**。
 */
function assertVersionFamilies(packages) {
  const families = new Map();
  for (const entry of packages) {
    const family = entry.manifest.versionFamily;
    if (!families.has(family)) families.set(family, []);
    families.get(family).push(entry);
  }
  for (const [family, entries] of families) {
    const versions = new Set(entries.map((entry) => entry.manifest.version));
    if (versions.size > 1) {
      const detail = entries
        .map((entry) => `${entry.manifest.name}=${entry.manifest.version}`)
        .join(', ');
      throw new Error(`版本族 ${family} 内版本不一致：${detail}`);
    }
    const expected = entries[0].manifest.version;
    for (const entry of entries) {
      // npm 包自身版本（若该包有 npm 产物）
      if (entry.manifest.npm?.name) {
        const npmDir = npmPackageDir(entry);
        const npmJsonPath = path.join(npmDir, 'package.json');
        if (fs.existsSync(npmJsonPath)) {
          const npmVersion = readJson(npmJsonPath).version;
          if (npmVersion !== expected) {
            throw new Error(
              `版本族 ${family} 内 ${entry.manifest.name} 的 npm 包版本 ${npmVersion} 与 manifest ${expected} 不一致`,
            );
          }
        }
      }
      // 平台包版本
      for (const platformPackage of entry.manifest.npm?.platformPackages ?? []) {
        const platformDir = platformPackageDir(entry, platformPackage);
        const platformJsonPath = path.join(platformDir, 'package.json');
        if (fs.existsSync(platformJsonPath)) {
          const platformVersion = readJson(platformJsonPath).version;
          if (platformVersion !== expected) {
            throw new Error(
              `版本族 ${family} 内平台包 ${platformPackage} 版本 ${platformVersion} 与 ${expected} 不一致`,
            );
          }
        }
      }
    }
  }
  return families;
}

/** 包内 npm 产物目录（npm/<name 去作用域>）。 */
function npmPackageDir(entry) {
  const npmName = entry.manifest.npm?.name;
  if (!npmName) return null;
  return path.join(entry.dir, 'npm', npmName.replace('@lania-cli-v2/', ''));
}

/** 平台包目录（与 npm 产物同级）。 */
function platformPackageDir(entry, platformPackageName) {
  const npmDir = npmPackageDir(entry);
  return path.join(path.dirname(npmDir), platformPackageName.replace('@lania-cli-v2/', ''));
}

function resolveHostPlatform() {
  return `${process.platform}-${process.arch}`;
}

function inferPlatformFromPackageName(packageName) {
  return packageName.replace('@lania-cli-v2/cli-', '');
}

function platformToEnvSuffix(platform) {
  return platform.replace(/[^A-Za-z0-9]+/g, '_').toUpperCase();
}

/**
 * 平台包描述符（文档 §6.2）：由 manifest 汇总而来。
 *
 * 遍历所有带 `api.rust.bin` 的包，取其 npm 平台包列表，
 * 平台信息（os/cpu）从平台包自身的 package.json 读取（manifest 里存名字）。
 */
function discoverPlatformPackageDescriptors(packages) {
  const descriptors = [];
  for (const entry of packages) {
    const bin = entry.manifest.api?.rust?.bin;
    if (!bin) continue;
    const platformPackages = entry.manifest.npm?.platformPackages ?? [];
    if (platformPackages.length === 0) continue;
    const npmDir = npmPackageDir(entry);
    for (const platformPackage of platformPackages) {
      const packageDirName = platformPackage.replace('@lania-cli-v2/', '');
      const packageRoot = path.join(path.dirname(npmDir), packageDirName);
      const packageJsonPath = path.join(packageRoot, 'package.json');
      const packageExists = fs.existsSync(packageJsonPath);
      const packageJson = packageExists ? readJson(packageJsonPath) : {};
      const os = Array.isArray(packageJson.os) ? packageJson.os[0] : null;
      const cpu = Array.isArray(packageJson.cpu) ? packageJson.cpu[0] : null;
      descriptors.push({
        packageName: platformPackage,
        packageRoot,
        packageDirName,
        packageExists,
        version: packageJson.version ?? entry.manifest.version,
        binaryName: bin,
        owner: entry.manifest.name,
        platform:
          typeof os === 'string' && typeof cpu === 'string'
            ? `${os}-${cpu}`
            : inferPlatformFromPackageName(platformPackage),
      });
    }
  }
  return descriptors.sort((left, right) => left.packageName.localeCompare(right.packageName));
}
function resolveInjectedPlatformBinaryPath(descriptor) {
  const suffix = platformToEnvSuffix(descriptor.platform);
  const candidates = [
    process.env[`LANIA_CLI_${suffix}_BINARY_SOURCE`],
    process.env[`LANIA_PLATFORM_BINARY_${suffix}`],
  ];
  const platformDir = process.env.LANIA_CLI_PLATFORM_BINARIES_DIR;
  if (typeof platformDir === 'string' && platformDir.trim().length > 0) {
    const root = platformDir.trim();
    candidates.push(
      path.join(root, descriptor.platform, 'lania-cli'),
      path.join(root, descriptor.platform, 'lania-cli.exe'),
      path.join(root, descriptor.platform, 'bin', 'lania-cli'),
      path.join(root, descriptor.platform, 'bin', 'lania-cli.exe'),
      path.join(root, descriptor.packageDirName, 'bin', 'lania-cli'),
      path.join(root, descriptor.packageName, 'bin', 'lania-cli'),
    );
  }
  for (const candidate of candidates) {
    if (typeof candidate !== 'string' || candidate.trim().length === 0) {
      continue;
    }
    if (fs.existsSync(candidate.trim())) {
      return candidate.trim();
    }
  }
  return null;
}

function stagePlatformBinary(descriptor, releaseBinaryPath, source) {
  if (!descriptor.packageExists) {
    console.warn(`[pack] skip ${descriptor.packageName}: package directory missing`);
    return {
      packageName: descriptor.packageName,
      platform: descriptor.platform,
      status: 'package_missing',
      source,
    };
  }
  // Windows 二进制必须带 `.exe`，否则 npm 安装后 spawn 会 ENOENT。
  const binaryBase = descriptor.binaryName ?? 'lania-cli';
  const targetName = descriptor.platform.startsWith('win32') ? `${binaryBase}.exe` : binaryBase;
  const dst = path.join(descriptor.packageRoot, 'bin', targetName);
  copyFile(releaseBinaryPath, dst, 0o755);
  assertStagedBinaryVersion(descriptor, dst);
  console.log(`[pack] staged ${descriptor.packageName} for ${descriptor.platform}`);
  return {
    packageName: descriptor.packageName,
    platform: descriptor.platform,
    status: 'staged',
    source,
  };
}

/**
 * 二进制里的版本号是编译期通过 `env!("CARGO_PKG_VERSION")` 烧进去的，
 * `assertVersionSync` 只比对 package.json，管不到它。
 *
 * 这里补一道守卫：只有「宿主平台」的二进制能真正执行并自报版本；
 * 其他平台只做来源标签校验（fetch 阶段已按 `v{version}` 拉取）。
 */
function assertStagedBinaryVersion(descriptor, binaryPath) {
  // 期望版本取自该包所属版本族的 manifest（文档 §6.5）。
  const expected = descriptor.version;
  if (descriptor.platform !== resolveHostPlatform()) {
    return;
  }
  const result = spawnSync(binaryPath, ['--version'], { encoding: 'utf8' });
  if (result.error) {
    throw new Error(
      `Failed to execute staged binary for ${descriptor.packageName}: ${result.error.message}`,
    );
  }
  const reported = String(result.stdout ?? '').trim();
  if (reported !== expected) {
    throw new Error(
      [
        `Platform binary version mismatch for ${descriptor.packageName}.`,
        `  binary reports : ${reported || '<empty>'}`,
        `  expected       : ${expected}`,
        '二进制是旧的：请先打 v{version} tag 让 CI 重新构建，再重新 pack。',
      ].join('\n'),
    );
  }
}

function stagePlatformBinaries(packages) {
  const hostPlatform = resolveHostPlatform();
  const results = [];
  for (const descriptor of discoverPlatformPackageDescriptors(packages)) {
    const binaryBase = descriptor.binaryName ?? 'lania-cli';
    const releaseBinaryPath = path.join(
      repoRoot,
      'rust/target/release',
      process.platform === 'win32' ? `${binaryBase}.exe` : binaryBase,
    );
    const injectedBinaryPath = resolveInjectedPlatformBinaryPath(descriptor);
    if (injectedBinaryPath) {
      results.push(stagePlatformBinary(descriptor, injectedBinaryPath, 'environment'));
      continue;
    }
    if (descriptor.platform !== hostPlatform) {
      console.log(
        `[pack] skip ${descriptor.packageName}: host ${hostPlatform} does not match ${descriptor.platform}`,
      );
      results.push({
        packageName: descriptor.packageName,
        platform: descriptor.platform,
        status: 'host_mismatch',
        source: 'host_platform',
      });
      continue;
    }
    if (!fs.existsSync(releaseBinaryPath)) {
      throw new Error(
        `Missing Rust release binary ${releaseBinaryPath}. Run cargo build --release first.`,
      );
    }
    results.push(stagePlatformBinary(descriptor, releaseBinaryPath, 'host_build'));
  }
  return results;
}
async function main() {
  const packages = discoverPackages();
  const families = assertVersionFamilies(packages);
  const lanFamily = families.get('lan');
  const lanVersion = lanFamily ? lanFamily[0].manifest.version : null;
  console.log(
    `[pack] 版本族：${[...families.entries()]
      .map(([family, entries]) => `${family}=${entries[0].manifest.version}`)
      .join(', ')}`,
  );

  if (process.argv.includes('--check')) {
    // 只做枚举与校验（CI 用）：不构建、不组装、不联网。
    console.log(
      `- packages: ${packages.length} 个；二进制 crate：${binaryCratesPreview(packages).join(', ')}`,
    );
    for (const entry of packages) {
      const payload = entry.manifest.payload ?? {};
      console.log(
        `  ${entry.manifest.kind.padEnd(15)} ${entry.manifest.name.padEnd(12)} ` +
          `v${entry.manifest.version} ${entry.manifest.api?.rust?.bin ?? '-'} ` +
          `${payload.nodeBridge ? `slices=[${(payload.slices ?? []).join(',')}]` : '零 payload'}`,
      );
    }
    return;
  }

  console.log('[pack] building TS workspace');
  runPnpm(['-r', 'build'], { cwd: repoRoot });

  // 一次 cargo build 覆盖所有产出二进制的包（共享 target/ 缓存，文档 §6.4）。
  const binaryCrates = [];
  for (const entry of packages) {
    const bin = entry.manifest.api?.rust?.bin;
    const lib = entry.manifest.api?.rust?.lib;
    if (!bin || !lib) continue;
    if (!binaryCrates.includes(lib)) binaryCrates.push(lib);
  }
  console.log(`[pack] building Rust binaries: ${binaryCrates.join(', ')}`);
  run(
    'cargo',
    ['build', '--release', '--manifest-path', 'rust/Cargo.toml', ...binaryCrates.flatMap((name) => ['-p', name])],
    { cwd: repoRoot },
  );

  // Optional: download prebuilt platform binaries (GitHub Releases) into staging dir
  // and expose them via LANIA_CLI_PLATFORM_BINARIES_DIR for staging into platform packages.
  await fetchPlatformBinaries().catch((error) => {
    console.warn(
      `[pack] fetch-platform-binaries failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  });

  console.log('[pack] staging per-package bridge payloads');
  const payloadResults = stagePackageBridgePayloads(packages);

  console.log('[pack] staging platform binary packages');
  const platformResults = stagePlatformBinaries(packages);

  console.log('[pack] done');
  console.log(
    `- npm packages: ${packages
      .filter((entry) => entry.manifest.npm?.name)
      .map((entry) => path.relative(repoRoot, npmPackageDir(entry)))
      .join(', ')}`,
  );
  console.log(
    `- bridge payloads: ${
      payloadResults.length > 0
        ? payloadResults.map((entry) => `${entry.packageName}:${entry.slices.join('+')}`).join(', ')
        : '（无）'
    }`,
  );
  console.log(
    `- platform staging: ${platformResults.map((entry) => `${entry.packageName}:${entry.status}`).join(', ')}`,
  );
  if (lanVersion) console.log(`- lan 版本族版本：${lanVersion}`);
}

/** 汇总所有产出二进制的 crate（--check 模式用）。 */
function binaryCratesPreview(packages) {
  const crates = [];
  for (const entry of packages) {
    const bin = entry.manifest.api?.rust?.bin;
    const lib = entry.manifest.api?.rust?.lib;
    if (!bin || !lib) continue;
    if (!crates.includes(lib)) crates.push(lib);
  }
  return crates;
}

/**
 * 按 manifest 的 `payload` 为每个包组装自己的 `lib/bridge/`（文档 §6.2）。
 *
 * - 核（bridge-runtime）始终随包；
 * - 切片按 `payload.slices` 精确挑选（总包为 ["*"] → 12 个全量）；
 * - 运行时依赖（ejs/tsx/yaml/pi-*）按需安装，不拖无关依赖。
 */
function stagePackageBridgePayloads(packages) {
  const results = [];
  const bridgeRuntimeDir = path.join(repoRoot, 'packages/foundation/bridge-runtime/ts');
  const templatesDir = path.join(repoRoot, 'packages/foundation/templates');
  const agentDir = path.join(repoRoot, 'packages/products/agent/ts/agent');
  const allSlices = fs.existsSync(path.join(repoRoot, 'packages/foundation/bridge-slices'))
    ? fs.readdirSync(path.join(repoRoot, 'packages/foundation/bridge-slices'))
    : [];

  for (const entry of packages) {
    const payload = entry.manifest.payload ?? {};
    if (!payload.nodeBridge) continue;
    const npmDir = npmPackageDir(entry);
    if (!npmDir) continue;
    const slices = Array.isArray(payload.slices) ? payload.slices : [];
    const selected = slices.includes('*') ? allSlices : slices;
    const target = path.join(npmDir, 'lib', 'bridge');
    rmIfExists(target);
    ensureDir(target);

    // 核：bridge-runtime（包名 @lania-cli-v2/bridge-runtime）
    const runtimeTarget = path.join(target, 'node_modules/@lania-cli-v2/bridge-runtime');
    ensureDir(runtimeTarget);
    copyFile(path.join(bridgeRuntimeDir, 'package.json'), path.join(runtimeTarget, 'package.json'));
    copyDir(path.join(bridgeRuntimeDir, 'dist'), path.join(runtimeTarget, 'dist'));

    // 切片：按 payload.slices 精确挑选
    for (const slice of selected) {
      const sliceSource = path.join(repoRoot, 'packages/foundation/bridge-slices', slice, 'ts');
      if (!fs.existsSync(sliceSource)) {
        throw new Error(`未知切片 ${slice}（来自 ${entry.manifest.name} 的 payload.slices）`);
      }
      const sliceTarget = path.join(target, `node_modules/@lania-cli-v2/bridge-slice-${slice}`);
      ensureDir(sliceTarget);
      copyFile(path.join(sliceSource, 'package.json'), path.join(sliceTarget, 'package.json'));
      copyDir(path.join(sliceSource, 'dist'), path.join(sliceTarget, 'dist'));
    }

    // TS 运行时依赖：templates（脚手架资产）与 agent（pi 引擎）
    const templatesTarget = path.join(target, 'node_modules/@lania-cli-v2/templates');
    ensureDir(templatesTarget);
    copyFile(path.join(templatesDir, 'package.json'), path.join(templatesTarget, 'package.json'));
    copyDir(path.join(templatesDir, 'dist'), path.join(templatesTarget, 'dist'));
    const agentTarget = path.join(target, 'node_modules/@lania-cli-v2/agent');
    ensureDir(agentTarget);
    copyFile(path.join(agentDir, 'package.json'), path.join(agentTarget, 'package.json'));
    copyDir(path.join(agentDir, 'dist'), path.join(agentTarget, 'dist'));

    // payload 自身的运行时依赖
    const bridgeRuntimePkg = readJson(path.join(bridgeRuntimeDir, 'package.json'));
    const templatesPkg = readJson(path.join(templatesDir, 'package.json'));
    const agentPkg = readJson(path.join(agentDir, 'package.json'));
    const payloadPkg = {
      name: '@lania-cli-v2/bridge-payload',
      private: true,
      type: 'module',
      dependencies: {
        ejs: templatesPkg.dependencies?.ejs ?? '^5.0.0',
        tsx: bridgeRuntimePkg.dependencies?.tsx ?? '^4.0.0',
        yaml: bridgeRuntimePkg.dependencies?.yaml ?? '^2.0.0',
        '@earendil-works/pi-agent-core': agentPkg.dependencies?.['@earendil-works/pi-agent-core'],
        '@earendil-works/pi-ai': agentPkg.dependencies?.['@earendil-works/pi-ai'],
      },
    };
    writeJson(path.join(target, 'package.json'), payloadPkg);
    run('npm', ['install', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund'], {
      cwd: target,
    });
    rmIfExists(path.join(target, 'package-lock.json'));
    results.push({ packageName: entry.manifest.npm.name, slices: selected });
  }
  return results;
}

await main();
