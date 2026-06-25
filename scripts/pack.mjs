import { spawnSync } from 'node:child_process';
import console from 'node:console';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { fetchPlatformBinaries } from './fetch-platform-binaries.mjs';

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

function assertVersionSync(expected) {
  const nodeBridge = readJson(path.join(repoRoot, 'ts/packages/node-bridge/package.json'));
  const templates = readJson(path.join(repoRoot, 'ts/packages/templates/package.json'));
  const cli = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  const versions = [
    ['rust workspace', expected],
    ['@lania-cli/node-bridge', nodeBridge.version],
    ['@lania-cli/templates', templates.version],
    ['@lania-cli/cli', cli.version],
  ];
  for (const descriptor of discoverPlatformPackageDescriptors()) {
    if (!descriptor.packageExists) {
      continue;
    }
    const platformPkg = readJson(path.join(descriptor.packageRoot, 'package.json'));
    versions.push([descriptor.packageName, platformPkg.version]);
  }
  const mismatched = versions.filter(([, v]) => v !== expected);
  if (mismatched.length > 0) {
    const detail = mismatched.map(([name, v]) => `${name}=${v}`).join(', ');
    throw new Error(
      `Version mismatch. Expected all packages to be ${expected}. Mismatched: ${detail}`,
    );
  }
}

function stageNodeBridgePayload() {
  const payloadRoot = path.join(repoRoot, 'npm/cli/lib/node-bridge');
  rmIfExists(payloadRoot);
  ensureDir(payloadRoot);

  const bridgeDist = path.join(repoRoot, 'ts/packages/node-bridge/dist');
  const templatesDist = path.join(repoRoot, 'ts/packages/templates/dist');

  const stdioEntry = path.join(bridgeDist, 'entry', 'stdio.js');
  const templatesEntry = path.join(templatesDist, 'index.js');
  if (!fs.existsSync(stdioEntry)) {
    throw new Error('Missing node-bridge dist. Run TS build first.');
  }
  if (!fs.existsSync(templatesEntry)) {
    throw new Error('Missing templates dist. Run TS build first.');
  }

  // Bridge runtime assets.
  copyFile(
    path.join(repoRoot, 'ts/packages/node-bridge/package.json'),
    path.join(payloadRoot, 'package.bridge.json'),
  );
  copyDir(bridgeDist, path.join(payloadRoot, 'dist'));

  // Templates runtime assets placed under node_modules so the bridge can import it by specifier.
  const templatesTarget = path.join(payloadRoot, 'node_modules/@lania-cli/templates');
  ensureDir(templatesTarget);
  copyFile(
    path.join(repoRoot, 'ts/packages/templates/package.json'),
    path.join(templatesTarget, 'package.json'),
  );
  copyDir(templatesDist, path.join(templatesTarget, 'dist'));

  // Install runtime deps into payloadRoot/node_modules so:
  // - `node --import tsx ...` resolves `tsx` from payloadRoot
  // - `@lania-cli/templates` can resolve `ejs`
  // - bridge can resolve `yaml`
  const nodeBridgePkg = readJson(path.join(repoRoot, 'ts/packages/node-bridge/package.json'));
  const templatesPkg = readJson(path.join(repoRoot, 'ts/packages/templates/package.json'));
  const payloadPkg = {
    name: '@lania-cli/node-bridge-payload',
    private: true,
    type: 'module',
    dependencies: {
      ejs: templatesPkg.dependencies?.ejs ?? '^5.0.0',
      tsx: nodeBridgePkg.dependencies?.tsx ?? '^4.0.0',
      yaml: nodeBridgePkg.dependencies?.yaml ?? '^2.0.0',
    },
  };
  writeJson(path.join(payloadRoot, 'package.json'), payloadPkg);

  run('npm', ['install', '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund'], {
    cwd: payloadRoot,
  });

  // Clean up files not needed at runtime.
  rmIfExists(path.join(payloadRoot, 'package-lock.json'));
}

function resolveHostPlatform() {
  return `${process.platform}-${process.arch}`;
}

function inferPlatformFromPackageName(packageName) {
  return packageName.replace('@lania-cli/cli-', '');
}

function platformToEnvSuffix(platform) {
  return platform.replace(/[^A-Za-z0-9]+/g, '_').toUpperCase();
}

function discoverPlatformPackageDescriptors() {
  const cli = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  const optionalDependencies = cli.optionalDependencies ?? {};
  return Object.entries(optionalDependencies)
    .filter(([packageName]) => packageName.startsWith('@lania-cli/cli-'))
    .map(([packageName, version]) => {
      const packageDirName = packageName.replace('@lania-cli/', '');
      const packageRoot = path.join(repoRoot, 'npm', packageDirName);
      const packageJsonPath = path.join(packageRoot, 'package.json');
      const packageExists = fs.existsSync(packageJsonPath);
      const packageJson = packageExists ? readJson(packageJsonPath) : {};
      const os = Array.isArray(packageJson.os) ? packageJson.os[0] : null;
      const cpu = Array.isArray(packageJson.cpu) ? packageJson.cpu[0] : null;
      return {
        packageName,
        packageRoot,
        packageDirName,
        packageExists,
        version: packageJson.version ?? version,
        platform:
          typeof os === 'string' && typeof cpu === 'string'
            ? `${os}-${cpu}`
            : inferPlatformFromPackageName(packageName),
      };
    })
    .sort((left, right) => left.packageName.localeCompare(right.packageName));
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
      path.join(root, descriptor.platform, 'bin', 'lania-cli'),
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
  const dst = path.join(descriptor.packageRoot, 'bin/lania-cli');
  copyFile(releaseBinaryPath, dst, 0o755);
  console.log(`[pack] staged ${descriptor.packageName} for ${descriptor.platform}`);
  return {
    packageName: descriptor.packageName,
    platform: descriptor.platform,
    status: 'staged',
    source,
  };
}

function stagePlatformBinaries() {
  const src = path.join(repoRoot, 'rust/target/release/lania-cli');
  if (!fs.existsSync(src)) {
    throw new Error('Missing Rust release binary. Run cargo build --release first.');
  }
  const hostPlatform = resolveHostPlatform();
  const results = [];
  for (const descriptor of discoverPlatformPackageDescriptors()) {
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
    results.push(stagePlatformBinary(descriptor, src, 'host_build'));
  }
  return results;
}

async function main() {
  const version = readRustWorkspaceVersion();
  assertVersionSync(version);

  console.log(`[pack] building TS workspace`);
  runPnpm(['-C', 'ts', 'install', '--no-frozen-lockfile'], { cwd: repoRoot });
  runPnpm(['-C', 'ts', 'build'], { cwd: repoRoot });

  console.log(`[pack] building Rust binary`);
  run('cargo', ['build', '--release', '-p', 'lania-cli', '--manifest-path', 'rust/Cargo.toml'], {
    cwd: repoRoot,
  });

  const descriptors = discoverPlatformPackageDescriptors();
  // Optional: download prebuilt platform binaries (GitHub Releases) into staging dir
  // and expose them via LANIA_CLI_PLATFORM_BINARIES_DIR for staging into platform packages.
  await fetchPlatformBinaries().catch((error) => {
    console.warn(
      `[pack] fetch-platform-binaries failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  });

  console.log(`[pack] staging node bridge payload for @lania-cli/cli`);
  stageNodeBridgePayload();

  console.log(`[pack] staging platform binary packages`);
  const platformResults = stagePlatformBinaries();

  console.log('[pack] done');
  console.log(
    `- npm packages: npm/cli, ${descriptors.map((entry) => entry.packageDirName).join(', ')}`,
  );
  console.log('- TS packages: ts/packages/templates, ts/packages/node-bridge');
  console.log(
    `- platform staging: ${platformResults.map((entry) => `${entry.packageName}:${entry.status}`).join(', ')}`,
  );
}

await main();
