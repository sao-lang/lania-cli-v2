#!/usr/bin/env node
/**
 * `@lania-cli-v2/core` 的唯一 `lan` 入口（文档 §5.3 C.1~C.7）。
 *
 * 两种运行模式：
 *   1. 总包模式（monolith）——检测到已安装的全量二进制即**直接 spawn**，零调度、零额外跳转；
 *   2. 调度模式（atomic）——只装 core + 若干 cmd-* 时，按构建期静态表 `known-commands.json`
 *      路由到对应的 `lan-<cmd>[.exe]`。
 *
 * 关键约束（§5.3 C.6）：
 * - `spawn(..., { stdio: 'inherit' })`：子进程继承真实 TTY，launcher 不插在 stdio 中间；
 * - 退出码原样透传；被信号杀死时把信号转发给自身；
 * - 全局参数（--json/--locale/--output-profile/--no-progress）不属于 core，由各命令二进制解析；
 * - launcher 只剥掉命令名，不改写其余 argv。
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const corePackageDir = path.resolve(scriptDir, '..');
// 开发模式下的仓库根（core 包在 packages/distribution/core/npm/core 下）。
const fallbackRepoRoot = path.resolve(corePackageDir, '..', '..', '..', '..');
const argv = process.argv.slice(2);

/** 当前平台对应的二进制包后缀（与总包 cli-<platform> 一致）。 */
function platformSuffix() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === 'darwin' && arch === 'arm64') return 'darwin-arm64';
  if (platform === 'linux' && arch === 'x64') return 'linux-x64';
  if (platform === 'win32' && arch === 'x64') return 'win32-x64';
  return null;
}

/** Windows 上二进制必须带 `.exe` 后缀，否则 spawn 直接 ENOENT。 */
function binaryFileName(base) {
  return process.platform === 'win32' ? `${base}.exe` : base;
}

/**
 * 解析某个平台包下的二进制绝对路径。
 *
 * `req` 决定解析上下文：平台包是**命令包/总包自己的 optionalDependencies**，
 * 因此必须从那个包的目录解析，而不是从 core 包解析。
 */
function resolveBinaryFromPlatformPackage(packageName, base, req = require) {
  try {
    const pkgDir = path.dirname(req.resolve(`${packageName}/package.json`));
    const bin = path.join(pkgDir, 'bin', binaryFileName(base));
    return fs.existsSync(bin) ? bin : null;
  } catch {
    return null;
  }
}

/** 从某个包目录构造 require（用于解析它的 optionalDependencies）。 */
function requireFrom(packageDir) {
  return createRequire(path.join(packageDir, 'package.json'));
}

/** Unix 上补可执行位（沿用旧 lan.mjs 的 ensureExecutable）。 */
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

/** 共用的 spawn 实现：两种模式行为一致（§5.3 C.6）。 */
function execDirect(binary, env, args) {
  ensureExecutable(binary);
  const child = spawn(binary, args, {
    stdio: 'inherit',
    env: { ...process.env, ...env },
  });
  child.on('error', (err) => {
    console.error(`Failed to launch ${binary}: ${err.message}`);
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

// ── 1) 总包直启 ───────────────────────────────────────────────────────

/** 开发模式：仓库内直接跑本地编译的 Rust 二进制。 */
function resolveSourceBinary() {
  const name = binaryFileName('lania-cli');
  const candidates = [
    path.join(fallbackRepoRoot, 'rust', 'target', 'debug', name),
    path.join(fallbackRepoRoot, 'rust', 'target', 'release', name),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

/**
 * 总包模式探测：已安装的 `@lania-cli-v2/cli` + 其平台包。
 * 这不是动态发现：用已知包名 `require.resolve` 探测，与旧 lan.mjs 同构。
 */
function resolveMonolithBinary() {
  if (process.env.LANIA_MONOLITH_BINARY) return process.env.LANIA_MONOLITH_BINARY;
  const suffix = platformSuffix();
  if (!suffix) return null;
  // 平台包是总包自己的 optionalDependencies：从总包目录解析。
  let cliDir = null;
  try {
    cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
  } catch {
    cliDir = null;
  }
  if (cliDir) {
    const binary = resolveBinaryFromPlatformPackage(
      `@lania-cli-v2/cli-${suffix}`,
      'lania-cli',
      requireFrom(cliDir),
    );
    if (binary) return binary;
  }
  return resolveSourceBinary();
}

/** 总包模式注入的环境变量（与旧 lan.mjs 逐行等价）。 */
function monolithEnv(cliDir) {
  const sourceMode = !cliDir;
  return {
    LANIA_NODE_BRIDGE_DIR:
      process.env.LANIA_NODE_BRIDGE_DIR ??
      (sourceMode
        ? path.join(fallbackRepoRoot, 'packages', 'foundation', 'bridge-runtime', 'ts')
        : path.join(cliDir, 'lib', 'node-bridge')),
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
    LANIA_RUNTIME_MODE:
      process.env.LANIA_RUNTIME_MODE ?? (sourceMode ? 'development' : 'installed'),
  };
}

// ── 2) 调度模式 ──────────────────────────────────────────────────────

/** 构建期生成的静态表（§5.3 C.7）：只读，不扫描目录。 */
function loadKnownCommands() {
  const file = path.join(corePackageDir, 'lib', 'known-commands.json');
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return { protocol: 1, coreVersion: '0.0.0', commands: [] };
  }
}

/** 已安装的命令包目录（未安装返回 null）。 */
function resolveCommandPackageDir(packageName) {
  try {
    return path.dirname(require.resolve(`${packageName}/package.json`));
  } catch {
    return null;
  }
}

/**
 * 命令包的平台二进制（cmd-<cmd>-<platform>）。
 * 平台包是命令包自己的 optionalDependencies，因此从命令包目录解析。
 */
function resolveCommandBinary(entry, cmdDir) {
  const suffix = platformSuffix();
  if (!suffix) return null;
  const platformPackage = `@lania-cli-v2/cmd-${entry.command}-${suffix}`;
  const listed = Array.isArray(entry.platformPackages) ? entry.platformPackages : [];
  if (listed.length > 0 && !listed.includes(platformPackage)) return null;
  return resolveBinaryFromPlatformPackage(platformPackage, entry.bin, requireFrom(cmdDir));
}

/** 调度模式的帮助聚合：已安装命令 + 未安装命令的安装提示。 */
function printAggregatedHelp(known) {
  const installed = [];
  const missing = [];
  for (const entry of known.commands) {
    if (resolveCommandPackageDir(entry.package)) installed.push(entry);
    else missing.push(entry);
  }
  console.log('Lania CLI v2 — 可用命令');
  console.log('');
  if (installed.length > 0) {
    console.log('已安装：');
    for (const entry of installed) {
      console.log(`  ${entry.command.padEnd(10)} ${entry.about ?? ''}`.trimEnd());
    }
    console.log('');
  }
  if (missing.length > 0) {
    console.log('未安装（可按需安装）：');
    for (const entry of missing) {
      console.log(`  ${entry.command.padEnd(10)} npm i ${entry.package}`);
    }
    console.log('');
  }
  console.log('用法：lan <命令> [参数]');
  console.log('查看单个命令帮助：lan <命令> --help');
}

function printVersion(known) {
  console.log(`Lania CLI v2 core ${known.coreVersion ?? '0.0.0'}`);
}

/** 校验协议版本（§9.11：运行时协议校验）。 */
function assertSupportedProtocol(protocol) {
  if (typeof protocol === 'number' && protocol > 1) {
    console.error(
      `当前 core 只支持 api.protocol <= 1，命令包声明 ${protocol}。请升级 @lania-cli-v2/core。`,
    );
    process.exit(1);
  }
}

function dispatch(known) {
  if (argv.length === 0 || ['--help', '-h', 'help'].includes(argv[0])) {
    printAggregatedHelp(known);
    return;
  }
  if (['--version', '-V'].includes(argv[0])) {
    printVersion(known);
    return;
  }

  const command = argv[0];
  const entry = known.commands.find((item) => item.command === command);
  if (!entry) {
    console.error(`未知命令 "${command}"。运行 lan --help 查看可用命令。`);
    process.exit(127);
  }

  const cmdDir = resolveCommandPackageDir(entry.package);
  if (!cmdDir) {
    console.error(`命令 "${command}" 未安装。安装：npm i ${entry.package}`);
    process.exit(127);
  }

  assertSupportedProtocol(entry.protocol ?? 1);

  const binary = resolveCommandBinary(entry, cmdDir);
  if (!binary) {
    console.error(
      `命令 "${command}" 的平台二进制缺失（${process.platform}-${process.arch}）。` +
        `请重新安装 ${entry.package}，或确认 optionalDependencies 已启用。`,
    );
    process.exit(1);
  }

  const env = {
    LANIA_COMMAND_NAME: entry.command,
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
    LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
  };
  if (entry.needsNodeBridge) {
    env.LANIA_NODE_BRIDGE_DIR = path.join(cmdDir, 'lib', 'bridge');
    env.LANIA_BRIDGE_SLICES = Array.isArray(entry.slices) ? entry.slices.join(',') : '';
  }

  // argv 契约：剥掉命令名后转发（§5.3 C.4）。
  execDirect(binary, env, argv.slice(1));
}

// ── 主入口 ────────────────────────────────────────────────────────────

function main() {
  const suffix = platformSuffix();
  const monolith = resolveMonolithBinary();
  if (monolith) {
    let cliDir = null;
    try {
      cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
    } catch {
      cliDir = null;
    }
    execDirect(monolith, monolithEnv(cliDir), argv);
    return;
  }

  if (!suffix) {
    console.error(
      `Unsupported platform: ${process.platform} ${process.arch}. ` +
        'Supported packaged platforms: darwin arm64, linux x64, win32 x64.',
    );
    process.exit(1);
  }

  dispatch(loadKnownCommands());
}

main();
