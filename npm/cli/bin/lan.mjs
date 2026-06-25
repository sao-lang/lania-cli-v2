#!/usr/bin/env node
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const fallbackCliPackageDir = path.resolve(scriptDir, '..');
const fallbackRepoRoot = path.resolve(fallbackCliPackageDir, '../..');

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
  const pkgJson = require.resolve(`${pkgName}/package.json`);
  const pkgDir = path.dirname(pkgJson);
  return path.join(pkgDir, 'bin', 'lania-cli');
}

function resolveCliPackageDir() {
  try {
    return {
      packageDir: path.dirname(require.resolve('@lania-cli/cli/package.json')),
      sourceMode: false,
    };
  } catch {
    // Support running the wrapper directly from the repository checkout.
    return {
      packageDir: fallbackCliPackageDir,
      sourceMode: true,
    };
  }
}

function resolveSourceBinaryPath() {
  const candidates = [
    path.join(fallbackRepoRoot, 'rust', 'target', 'debug', 'lania-cli'),
    path.join(fallbackRepoRoot, 'rust', 'target', 'release', 'lania-cli'),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

function resolveRuntimeLayout() {
  const { packageDir, sourceMode } = resolveCliPackageDir();
  if (sourceMode) {
    return {
      sourceMode,
      packageDir,
      productRoot: fallbackRepoRoot,
      bridgeDir: path.join(fallbackRepoRoot, 'ts', 'packages', 'node-bridge'),
      binaryPath: resolveSourceBinaryPath(),
    };
  }
  return {
    sourceMode,
    packageDir,
    productRoot: packageDir,
    bridgeDir: path.join(packageDir, 'lib', 'node-bridge'),
    binaryPath: null,
  };
}

function ensureExecutable(binaryPath) {
  try {
    fs.accessSync(binaryPath, fs.constants.X_OK);
  } catch {
    const stat = fs.statSync(binaryPath);
    if (!stat.isFile()) {
      throw new Error(`Platform binary is not a file: ${binaryPath}`);
    }
    fs.chmodSync(binaryPath, stat.mode | 0o755);
  }
}

function main() {
  const runtime = resolveRuntimeLayout();

  let binaryPath = runtime.binaryPath;
  if (!binaryPath) {
    const binPkg = resolvePlatformBinaryPackage();
    if (!binPkg) {
      console.error(
        `Unsupported platform: ${process.platform} ${process.arch}. Supported packaged platforms: darwin arm64, linux x64.`,
      );
      process.exit(1);
    }

    try {
      binaryPath = resolveBinaryPath(binPkg);
    } catch (err) {
      console.error(
        `Missing optional binary dependency ${binPkg}. Reinstall the package, or ensure optionalDependencies are enabled.`,
      );
      process.exit(1);
    }
  }

  const productRoot = runtime.productRoot;
  const bridgeDir = runtime.bridgeDir;
  ensureExecutable(binaryPath);
  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
    env: {
      ...process.env,
      LANIA_NODE_BRIDGE_DIR: process.env.LANIA_NODE_BRIDGE_DIR ?? bridgeDir,
      LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? productRoot,
      LANIA_RUNTIME_MODE:
        process.env.LANIA_RUNTIME_MODE ?? (runtime.sourceMode ? 'development' : 'installed'),
    },
  });
  child.on('error', (err) => {
    console.error(`Failed to launch ${binaryPath}: ${err.message}`);
    process.exit(1);
  });
  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

main();
