#!/usr/bin/env node

// Lania CLI 入口脚本，负责定位并启动平台原生二进制文件。
// 支持两种运行模式：
//   1. installed  —— 通过 npm 安装后运行，使用预编译的平台二进制包。
//   2. development —— 在仓库源码中直接运行，使用本地编译的 Rust 二进制。

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

// 在 ESM 模块中模拟 CommonJS 的 require，用于解析 npm 包路径。
const require = createRequire(import.meta.url);
// 当前脚本所在目录。
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
// 开发模式下的回退路径：cli 包的根目录。
const fallbackCliPackageDir = path.resolve(scriptDir, '..');
// 开发模式下的回退路径：整个仓库的根目录。
const fallbackRepoRoot = path.resolve(fallbackCliPackageDir, '../..');

// 根据当前平台和架构返回对应的 npm 二进制包名。
// 仅支持 darwin-arm64 和 linux-x64。
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

// 解析指定 npm 包中平台二进制文件的绝对路径。
function resolveBinaryPath(pkgName) {
  const pkgJson = require.resolve(`${pkgName}/package.json`);
  const pkgDir = path.dirname(pkgJson);
  return path.join(pkgDir, 'bin', 'lania-cli');
}

// 解析 cli 包的根目录路径，同时判断当前运行模式。
// 优先尝试 @lania-cli/cli（已安装模式），失败则回退到仓库源码路径（开发模式）。
function resolveCliPackageDir() {
  try {
    return {
      packageDir: path.dirname(require.resolve('@lania-cli/cli/package.json')),
      sourceMode: false,
    };
  } catch {
    // 未找到已安装的包，回退到仓库源码路径。
    return {
      packageDir: fallbackCliPackageDir,
      sourceMode: true,
    };
  }
}

// 在开发模式下查找本地编译的 Rust 二进制文件。
// 优先使用 debug 构建，其次 release 构建。
function resolveSourceBinaryPath() {
  const candidates = [
    path.join(fallbackRepoRoot, 'rust', 'target', 'debug', 'lania-cli'),
    path.join(fallbackRepoRoot, 'rust', 'target', 'release', 'lania-cli'),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

// 统一解析运行时所需的所有路径和模式信息。
// 返回：sourceMode, packageDir, productRoot, bridgeDir, binaryPath。
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

// 确保二进制文件具有可执行权限（Unix 系统）。
// 如果文件不可执行，则添加 0o755 权限位。
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

// 主入口：解析运行时环境，定位二进制文件，设置环境变量并启动子进程。
function main() {
  const runtime = resolveRuntimeLayout();

  // 如果已安装模式未找到二进制路径，则尝试通过平台二进制包解析。
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

  // 确保二进制文件可执行。
  ensureExecutable(binaryPath);

  // 启动原生二进制子进程，转发命令行参数。
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

  // 子进程启动失败时的错误处理。
  child.on('error', (err) => {
    console.error(`Failed to launch ${binaryPath}: ${err.message}`);
    process.exit(1);
  });

  // 子进程退出时同步退出码或转发信号。
  child.on('exit', (code, signal) => {
    if (signal) {
      // 将收到的信号转发给当前进程。
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

main();