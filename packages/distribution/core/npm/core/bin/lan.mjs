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
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const corePackageDir = path.resolve(scriptDir, '..');
// 开发模式下的仓库根（core 包在 packages/distribution/core/npm/core 下，需上溯 5 级）。
const fallbackRepoRoot = path.resolve(corePackageDir, '..', '..', '..', '..', '..');
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

/**
 * 从某个包目录构造 require（用于解析它的 optionalDependencies）。
 */
function requireFrom(packageDir) {
  return createRequire(path.join(packageDir, 'package.json'));
}

/**
 * 在候选目录里查找某个 npm 包（逐个尝试）。
 *
 * pnpm 的严格 node_modules 布局下，包只能被「声明了它」的包解析到：
 * - `core` 直接依赖的 `cmd-*`（原子安装形态）；
 * - `cli`（总包）依赖的 `cmd-*`（总包安装形态）。
 * 因此候选目录必须覆盖这两种来源。
 */
function resolvePackageIn(candidates, packageName) {
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      return path.dirname(requireFrom(candidate).resolve(`${packageName}/package.json`));
    } catch {
      continue;
    }
  }
  return null;
}

/** 候选解析目录：core 自身 + 已安装的总包。 */
function packageResolutionRoots() {
  const roots = [corePackageDir];
  try {
    roots.push(path.dirname(require.resolve('@lania-cli-v2/cli/package.json')));
  } catch {
    // 总包未安装：只有 core 自身这一处来源。
  }
  return roots;
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
    path.join(fallbackRepoRoot, 'target', 'debug', name),
    path.join(fallbackRepoRoot, 'target', 'release', name),
  ];
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
}

/**
 * 旧单体二进制的探测（仅用于 `LANIA_MONOLITH_BINARY` 未显式指定时的开发态回退）。
 *
 * 总包已不再静态链接全部命令，正常路径一律走调度模式；这里保留是为了
 * 开发者在仓库内直接跑 `cargo build` 出来的单体二进制做对照调试。
 */
function resolveMonolithBinary() {
  if (process.env.LANIA_MONOLITH_BINARY) return process.env.LANIA_MONOLITH_BINARY;
  return resolveSourceBinary();
}

/**
 * 总包模式注入的环境变量（与旧 lan.mjs 逐行等价）。
 *
 * 注：总包已不再自带 bridge payload（切片由各命令子包自备），
 * 故 NODE_BRIDGE_DIR 只在显式指定时透传；否则由调度模式按子包注入。
 */
function monolithEnv(cliDir) {
  const sourceMode = !cliDir;
  const env = {
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
    LANIA_RUNTIME_MODE:
      process.env.LANIA_RUNTIME_MODE ?? (sourceMode ? 'development' : 'installed'),
  };
  if (process.env.LANIA_NODE_BRIDGE_DIR) {
    env.LANIA_NODE_BRIDGE_DIR = process.env.LANIA_NODE_BRIDGE_DIR;
  }
  return env;
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

/**
 * 已安装的命令包目录（未安装返回 null）。
 *
 * 解析上下文有两处：
 * 1. core 包自身（用户直接装 `core` + 若干 `cmd-*` 的「原子安装」形态）；
 * 2. 总包 `@lania-cli-v2/cli` 目录（用户装总包形态——子包是**总包**的依赖，
 *    不是 core 的依赖，故必须从总包目录解析）。
 */
function resolveCommandPackageDir(packageName) {
  return resolvePackageIn(packageResolutionRoots(), packageName);
}

/**
 * 命令包的平台二进制（cmd-<cmd>-<platform>）。
 * 平台包是命令包自己的 optionalDependencies，因此从命令包目录解析。
 *
 * 开发态回退：仓库内 `cargo build` 产出的二进制在 `target/debug/`，
 * 未经 pack 流程 staging 时平台包里没有二进制，此时直接用本地构建产物。
 */
function resolveCommandBinary(entry, cmdDir) {
  const suffix = platformSuffix();
  if (!suffix) return null;
  const platformPackage = `@lania-cli-v2/cmd-${entry.command}-${suffix}`;
  const listed = Array.isArray(entry.platformPackages) ? entry.platformPackages : [];
  if (listed.length > 0 && !listed.includes(platformPackage)) return null;
  const staged = resolveBinaryFromPlatformPackage(platformPackage, entry.bin, requireFrom(cmdDir));
  if (staged) return staged;
  // 开发态回退：仓库内本地构建产物。
  const local = path.join(fallbackRepoRoot, 'target', 'debug', binaryFileName(entry.bin));
  return fs.existsSync(local) ? local : null;
}

/**
 * 调度模式的帮助聚合。
 *
 * 契约（用户期望 §3）：**只有已安装命令子包才出现在命令清单里**；
 * 未安装的命令不出现在命令信息中，仅在末尾给一行「可按需安装」提示，
 * 具体清单由 `lan package list` 查询。
 */
function printAggregatedHelp(known) {
  const states = loadPackageStates();
  const installed = [];
  let missingCount = 0;
  for (const entry of known.commands) {
    const present = resolveCommandPackageDir(entry.package) !== null;
    // 未安装或已禁用的命令都不出现在命令清单里。
    if (present && !isPackageDisabled(entry.command, states)) installed.push(entry);
    else missingCount += 1;
  }
  console.log('Lania CLI v2 — 可用命令');
  console.log('');
  // config 是主包内建命令（不在子包表），固定追加在清单末尾。
  const builtinRows = [
    {
      command: 'config',
      about: tr('Manage global CLI preferences', '管理全局 CLI 配置与项目 lan.config'),
    },
  ];
  for (const entry of installed) {
    const about = tr(entry.about ?? '', entry.aboutZh ?? entry.about ?? '');
    console.log(`  ${entry.command.padEnd(10)} ${about}`.trimEnd());
  }
  for (const row of builtinRows) {
    console.log(`  ${row.command.padEnd(10)} ${row.about}`.trimEnd());
  }
  console.log('');
  if (installed.length > 0) {
    console.log('');
  } else {
    console.log('  （尚未安装任何命令子包）');
    console.log('');
  }
  console.log('用法：lan <命令> [参数]');
  console.log('查看单个命令帮助：lan <命令> --help');
  console.log('管理命令子包：lan package list');
  if (missingCount > 0) {
    console.log(`另有 ${missingCount} 个命令子包未安装，运行 lan package list 查看并按需安装。`);
  }
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

// ── Phase 2：动态回退（文档 §5.6 F.2）───────────────────────────────

/** runtime 设施是否已安装（core 的 optional/peer 依赖，缺失即提示）。 */
function resolveRuntimeDir() {
  try {
    return path.dirname(require.resolve('@lania-cli-v2/runtime/package.json'));
  } catch {
    return null;
  }
}

/** runtime 设施的平台二进制（runtime-<platform>）。 */
function resolveRuntimeBinary(runtimeDir) {
  const suffix = platformSuffix();
  if (!suffix) return null;
  return resolveBinaryFromPlatformPackage(
    `@lania-cli-v2/runtime-${suffix}`,
    'lan-runtime',
    requireFrom(runtimeDir),
  );
}

/**
 * 询问 runtime 设施：项目里是否挂了这条动态命令。
 * 返回 { found, handlerId }；设施不可用时返回 null（调用方据此给安装提示）。
 */
function askRuntime(runtimeBinary, command, cwd) {
  try {
    const result = spawnSync(runtimeBinary, ['--resolve', command, '--cwd', cwd], {
      encoding: 'utf8',
      env: { ...process.env, LANIA_COMMAND_NAME: command },
    });
    if (result.status !== 0 || !result.stdout) return { found: false };
    const payload = JSON.parse(result.stdout);
    return { found: payload.found === true, handlerId: payload.handlerId ?? null };
  } catch {
    return { found: false };
  }
}

/** Phase 2：把动态命令交给 runtime 设施执行。 */
function dispatchDynamic(command, args) {
  const runtimeDir = resolveRuntimeDir();
  if (!runtimeDir) {
    console.error(
      `命令 "${command}" 既不是内置命令，也未安装 runtime 设施。\n` +
        '  项目级动态命令需要：npm i @lania-cli-v2/runtime',
    );
    process.exit(127);
  }
  const runtimeBinary = resolveRuntimeBinary(runtimeDir);
  if (!runtimeBinary) {
    console.error(
      `runtime 设施的平台二进制缺失（${process.platform}-${process.arch}）。` +
        '  请重新安装 @lania-cli-v2/runtime。',
    );
    process.exit(1);
  }
  const cwd = process.cwd();
  const verdict = askRuntime(runtimeBinary, command, cwd);
  if (!verdict.found) {
    console.error(`未知命令 "${command}"。运行 lan --help 查看可用命令。`);
    process.exit(127);
  }
  const env = {
    LANIA_COMMAND_NAME: command,
    LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? cwd,
    LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
  };
  if (process.env.LANIA_NODE_BRIDGE_DIR) {
    env.LANIA_NODE_BRIDGE_DIR = process.env.LANIA_NODE_BRIDGE_DIR;
  }
  // 动态命令由 runtime 设施执行（同一份 node-bridge + host-core）。
  execDirect(runtimeBinary, env, [command, ...args]);
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

  // `package` 是 launcher 内建的子包管理命令（不来自任何子包）。
  if (command === 'package') {
    packageCommand(known, argv.slice(1));
    return;
  }

  // `config packages` / `config apply` 是 launcher 内建的跨包配置管理命令。
  // 其余 `config` 子命令（get/set/init/doctor/sync）由 config 子包提供。
  if (command === 'config') {
    const subcommand = argv[1];
    if (subcommand === 'packages') {
      configPackagesCommand(known, argv.slice(2));
      return;
    }
    if (subcommand === 'apply') {
      configApplyCommand(known, argv.slice(2));
      return;
    }
    // `--package` 允许 npm 全名 / bin 名 / crate 名；转发给 config 子包的 Rust
    // 二进制前统一成短命名空间，保证主包写入的正是子包真正读取的那份文件。
    const flag = argv.findIndex((item) => item === '--package' || item === '-p');
    const rawPackage = flag >= 0 ? argv[flag + 1] : null;
    if (rawPackage && !rawPackage.startsWith('-')) {
      const normalized = normalizePackageName(rawPackage, known);
      if (!normalized) {
        console.error(tr(`Unknown package: ${rawPackage}`, `未知子包：${rawPackage}`));
        process.exit(1);
      }
      argv[flag + 1] = normalized;
    }
  }

  // 二级命名空间路由：`lan product <sub>` 这类命令不在 known-commands 顶级表里，
  // 由 NESTED_ROUTES 指明承载子包；argv **完整保留**（含顶级名）——子包内已挂载
  // 对应命名空间 root（如 workspace 包内同时有 workspace.build 与 product.build，
  // 保留 'product' 才能与 `lan workspace build` 区分开）。
  const nestedTarget = NESTED_ROUTES[command];
  if (Array.isArray(nestedTarget)) {
    const nestedEntry = known.commands.find((item) => item.command === nestedTarget[0]);
    if (nestedEntry && nestedTarget.includes(argv[1])) {
      const nestedDir = resolveCommandPackageDir(nestedEntry.package);
      if (!nestedDir) {
        console.error(`命令 "${command}" 的承载子包未安装。安装：npm i ${nestedEntry.package}`);
        process.exit(1);
      }
      assertSupportedProtocol(nestedEntry.protocol ?? 1);
      const nestedBinary = resolveCommandBinary(nestedEntry, nestedDir);
      if (!nestedBinary) {
        console.error(
          `命令 "${command}" 的平台二进制缺失（${process.platform}-${process.arch}）。` +
            `请重新安装 ${nestedEntry.package}。`,
        );
        process.exit(1);
      }
      const nestedEnv = {
        LANIA_COMMAND_NAME: nestedEntry.command,
        LANIA_PRODUCT_ROOT: process.env.LANIA_PRODUCT_ROOT ?? process.cwd(),
        LANIA_RUNTIME_MODE: process.env.LANIA_RUNTIME_MODE ?? 'installed',
      };
      if (nestedEntry.needsNodeBridge) {
        nestedEnv.LANIA_NODE_BRIDGE_DIR = path.join(nestedDir, 'lib', 'bridge');
        nestedEnv.LANIA_BRIDGE_SLICES = Array.isArray(nestedEntry.slices)
          ? nestedEntry.slices.join(',')
          : '';
      }
      execDirect(nestedBinary, nestedEnv, argv.slice(1));
      return;
    }
  }

  // `config` 是主包内建命令（总包二进制承载）：packages/apply 已由 launcher
  // 内建处理，其余子命令整体转发总包二进制（argv 完整保留）。
  if (command === 'config') {
    const mainBinary = resolveMainCliBinary();
    if (mainBinary) {
      execDirect(mainBinary, {}, argv.slice(1));
      return;
    }
    console.error(
      'config 由总包二进制承载，但未找到 lania-cli 平台二进制。请安装 @lania-cli-v2/cli。',
    );
    process.exit(1);
  }

  const entry = known.commands.find((item) => item.command === command);
  if (!entry) {
    // Phase 1 未命中 → Phase 2 动态回退（§5.6 F.2）：项目级动态命令由 runtime 设施承载。
    dispatchDynamic(command, argv.slice(1));
    return;
  }

  // 被显式禁用的子包：不执行（等同于未安装）。
  if (isPackageDisabled(command, loadPackageStates())) {
    console.error(`命令 "${command}" 已被禁用（lan package enable ${command} 可重新启用）。`);
    process.exit(127);
  }

  const cmdDir = resolveCommandPackageDir(entry.package);
  if (!cmdDir) {
    // builtin 未安装：先问 runtime 设施（项目可能挂了同名动态命令），再给安装提示。
    const runtimeDir = resolveRuntimeDir();
    if (runtimeDir) {
      const runtimeBinary = resolveRuntimeBinary(runtimeDir);
      if (runtimeBinary) {
        const verdict = askRuntime(runtimeBinary, command, process.cwd());
        if (verdict.found) {
          dispatchDynamic(command, argv.slice(1));
          return;
        }
      }
    }
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

/** 主包内建 config 的承载二进制：开发态 source 二进制优先，其次总包平台包。 */
function resolveMainCliBinary() {
  const source = resolveSourceBinary();
  if (source) return source;
  try {
    const cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
    const suffix = `${process.platform}-${process.arch}`;
    const platformDir = path.join(path.dirname(cliDir), `cli-${suffix}`);
    const platformPkg = JSON.parse(fs.readFileSync(path.join(platformDir, 'package.json'), 'utf8'));
    const bin = Object.keys(platformPkg.bin ?? {})[0] ?? 'lania-cli';
    const candidate = path.join(
      platformDir,
      'bin',
      process.platform === 'win32' ? `${bin}.exe` : bin,
    );
    return fs.existsSync(candidate) ? candidate : null;
  } catch {
    return null;
  }
}

// ── 二级命名空间路由表 ────────────────────────────────────────────────
// `lan product <sub>` / `lan workspace <sub>` 的二级命令由承载子包提供。
// 值数组为允许的子命令集合（防止把未知二级命令误路由）。
const NESTED_ROUTES = {
  product: ['dev', 'build', 'pack', 'publish', 'inspect', 'doctor'],
  workspace: ['dev', 'build', 'lint', 'create', 'add', 'template', 'config'],
};

// ── 3) 命令子包管理（lan package）────────────────────────────────────
//
// 总包的角色是「管理命令子包」：命令能力全部由子包提供，
// 未安装子包的命令不可用。本命令提供子包的增删改查：
//   lan package list                列出全部子包及安装状态
//   lan package install <cmd>       安装子包
//   lan package uninstall <cmd>     卸载子包
//   lan package update <cmd>        更新子包
//   lan package enable/disable <cmd> 启用/禁用（写配置，不动 node_modules）
//
// 子包清单来自构建期静态表 known-commands.json（不扫描目录）。

/** 子包的启用/禁用状态文件（与 lania-package-config 的 per-pkg 存储一致）。 */
function packageStatePath() {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) return null;
  return path.join(home, '.lania', 'packages.json');
}

function loadPackageStates() {
  const file = packageStatePath();
  if (!file) return {};
  try {
    const raw = fs.readFileSync(file, 'utf8');
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function savePackageStates(states) {
  const file = packageStatePath();
  if (!file) return;
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(states, null, 2)}\n`, 'utf8');
}

/** 子包是否被显式禁用。 */
function isPackageDisabled(command, states) {
  return states[command]?.disabled === true;
}

/** 在当前工作目录（或全局）执行 npm。 */
function runNpm(args, cwd) {
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  const result = spawnSync(npm, args, {
    cwd,
    stdio: 'inherit',
    shell: process.platform === 'win32',
    env: { ...process.env, npm_config_yes: 'true' },
  });
  return result.status ?? 1;
}

/** 找到命令子包条目；未找到时给出可安装清单提示并退出。 */
function requireCommandEntry(known, command) {
  const entry = known.commands.find((item) => item.command === command);
  if (!entry) {
    console.error(`未知命令子包 "${command}"。`);
    const names = known.commands.map((item) => item.command).join(', ');
    console.error(`可用子包：${names}`);
    process.exit(1);
  }
  return entry;
}

function printPackageList(known) {
  const states = loadPackageStates();
  const rows = known.commands.map((entry) => {
    const installed = resolveCommandPackageDir(entry.package) !== null;
    const disabled = isPackageDisabled(entry.command, states);
    let status;
    if (!installed) status = '未安装';
    else if (disabled) status = '已禁用';
    else status = '已安装';
    return {
      command: entry.command,
      package: entry.package,
      status,
      about: tr(entry.about ?? '', entry.aboutZh ?? entry.about ?? ''),
    };
  });
  console.log('命令子包：');
  console.log('');
  for (const row of rows) {
    console.log(`  ${row.command.padEnd(10)} ${row.status.padEnd(8)} ${row.package}`);
    if (row.about) console.log(`  ${' '.repeat(10)} ${row.about}`);
  }
  console.log('');
  const installedCount = rows.filter((row) => row.status === '已安装').length;
  console.log(`共 ${rows.length} 个子包，已启用 ${installedCount} 个。`);
}

// ── 4) 配置管理（lan config packages / apply）─────────────────────────
//
// 总包视角的配置管理：跨包查看各命令子包的包级偏好，并把全局配置
// 一键覆盖下发到全部已安装子包。子包偏好存放在
// `~/.lania/config/<pkg>.json`（与 Rust 侧 lania-package-config 引擎同路径）。

/** launcher 侧的文案按全局偏好 locale 选择语言。 */
function tr(en, zh) {
  return loadGlobalPreferences().locale === 'zh' ? zh : en;
}

/**
 * 把用户可能写出的多种包名形态统一成 config 文件的短命名空间。
 *
 * 支持短命令名（env）、npm 包名（@lania-cli-v2/cmd-env / cmd-env）、
 * 二进制名（lan-env）、crate 名（lania-plugins-command-env）与平台包名。
 * 未命中静态表时返回 null，由调用方报错——避免把任意字符串写成配置文件。
 */
function normalizePackageName(input, known) {
  const raw = String(input ?? '').trim();
  if (!raw) return null;
  if (known.commands.some((item) => item.command === raw)) return raw;

  const candidates = known.commands.filter((item) => {
    const short = item.command;
    const aliases = [
      item.package,
      typeof item.package === 'string' ? item.package.replace(/^@[^/]+\//, '') : null,
      `lan-${short}`,
      `lania-plugins-command-${short}`,
      ...(item.platformPackages ?? []),
      ...(item.platformPackages ?? []).map((name) =>
        typeof name === 'string' ? name.replace(/^@[^/]+\//, '') : null,
      ),
    ];
    return aliases.includes(raw);
  });
  return candidates.length === 1 ? candidates[0].command : null;
}

/** 子包偏好文件路径（与 Rust 侧 lania-package-config 引擎一致）。 */
function packageConfigPath(pkg) {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) return null;
  return path.join(home, '.lania', 'config', `${pkg}.json`);
}

function loadPackageConfig(pkg) {
  const file = packageConfigPath(pkg);
  if (!file) return {};
  try {
    const parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function savePackageConfig(pkg, config) {
  const file = packageConfigPath(pkg);
  if (!file) throw new Error('无法确定 HOME 目录，子包偏好未写入');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
  return file;
}

/** 点分 key 读写（与 Rust 侧 set_value / get_value 同语义）。 */
function setDottedValue(target, dottedKey, value) {
  const segments = dottedKey.split('.');
  if (segments.some((segment) => segment.trim() === '')) {
    throw new Error(`invalid config key \`${dottedKey}\``);
  }
  let current = target;
  for (let index = 0; index < segments.length; index += 1) {
    const segment = segments[index];
    if (index + 1 === segments.length) {
      current[segment] = value;
      return;
    }
    if (typeof current[segment] !== 'object' || current[segment] === null) {
      current[segment] = {};
    }
    current = current[segment];
  }
}

/**
 * 全局配置 → 子包偏好的覆盖映射。
 *
 * 只覆盖「子包确实声明了该键」的情况：子包的键集合由各自 manifest 的
 * `configSchema.keys[].key` 给出。这里从子包清单读取该声明，
 * 避免把全局键无差别灌进每个子包（那会污染子包的配置语义）。
 */
function globalOverridesFor(globalConfig, schemaKeys, explicit = {}) {
  const overrides = {};
  // 子包 configSchema 用点分键（output.mode），全局偏好落盘是 camelCase
  // （outputMode）。这里做一次显式映射，否则 apply 永远匹配不到这两个键。
  const aliases = {
    'output.mode': ['outputMode', 'output.mode', 'output'],
    'log.timestamps': ['logTimestamps', 'log.timestamps', 'log.timestamp'],
    locale: ['locale'],
  };
  for (const key of schemaKeys) {
    const names = aliases[key] ?? [key];
    const hit = names.find((name) => globalConfig[name] !== undefined);
    if (hit) overrides[key] = globalConfig[hit];
    if (explicit[key] !== undefined) overrides[key] = explicit[key];
  }
  return overrides;
}

/**
 * 从命令包读取 configSchema 的键列表。
 *
 * `configSchema` 声明在包的 `lania.package.json`（仓库内的清单）里，
 * 发布后的 npm 包不一定带它，故先找 npm 包自身的 manifest，
 * 再向上找同级的 `lania.package.json`；两者都没有则返回空数组
 * （此时 apply 会跳过该子包，不做无差别覆盖）。
 */
function readPackageConfigSchema(entry, cmdDir) {
  // 构建期静态表是唯一事实源：同时装了主包与子包时，launcher 无需再解析磁盘上的
  // npm manifest 就知道每个子包声明了哪些键。
  if (Array.isArray(entry?.configSchema?.keys)) return entry.configSchema.keys;
  const candidates = cmdDir
    ? [path.join(cmdDir, 'package.json'), path.join(cmdDir, '..', '..', 'lania.package.json')]
    : [];
  for (const candidate of candidates) {
    try {
      const manifest = JSON.parse(fs.readFileSync(candidate, 'utf8'));
      const schema = manifest.configSchema ?? manifest.lania?.configSchema;
      if (Array.isArray(schema?.keys)) return schema.keys;
    } catch {
      continue;
    }
  }
  return [];
}

function schemaKeyNames(keys) {
  return keys.map((item) => (typeof item === 'string' ? item : item?.key)).filter(Boolean);
}

/** 点分 key 读取（与 setDottedValue 对称）。 */
function getDottedValue(source, dottedKey) {
  let current = source;
  for (const segment of dottedKey.split('.')) {
    if (!current || typeof current !== 'object') return undefined;
    current = current[segment];
  }
  return current;
}

/**
 * 跨包配置视图。`config` 自身是管理面，不作为被管理的子包出现。
 * 展示的是「子包声明了哪些键 + 当前值 + 默认值」，而不只是已落盘的键。
 */
function configPackagesCommand(known, args) {
  const states = loadPackageStates();
  const rows = [];
  for (const entry of known.commands) {
    if (entry.command === 'config') continue;
    const cmdDir = resolveCommandPackageDir(entry.package);
    if (!cmdDir) continue;
    if (isPackageDisabled(entry.command, states)) continue;
    const schema = readPackageConfigSchema(entry, cmdDir);
    const config = loadPackageConfig(entry.command);
    rows.push({
      command: entry.command,
      package: entry.package,
      path: packageConfigPath(entry.command),
      keys: schemaKeyNames(schema).map((key) => {
        const declared = schema.find((item) => (item?.key ?? item) === key) ?? {};
        return {
          key,
          kind: declared.kind ?? null,
          default: declared.default ?? null,
          value: getDottedValue(config, key) ?? null,
        };
      }),
    });
  }

  if (args.includes('--json')) {
    console.log(JSON.stringify({ scope: 'packages', packages: rows }, null, 2));
    return;
  }
  console.log(tr('Command package configuration:', '命令子包配置：'));
  console.log('');
  if (rows.length === 0) {
    console.log(tr('  (no installed command subpackages)', '  （没有已安装的命令子包）'));
    return;
  }
  for (const row of rows) {
    console.log(`  ${row.command}  (${row.package})`);
    if (row.keys.length === 0) {
      console.log(tr('    (no configSchema declared)', '    （未声明 configSchema）'));
    } else {
      for (const item of row.keys) {
        const current = item.value === null ? (item.default ?? '-') : JSON.stringify(item.value);
        console.log(`    ${item.key.padEnd(18)} = ${current}`);
      }
    }
    console.log(`    ${row.path}`);
  }
  console.log('');
  console.log(tr(`Total: ${rows.length} subpackages.`, `共 ${rows.length} 个子包。`));
  console.log(
    tr('Read:  lan config get <key> --package <pkg>', '读取：lan config get <key> --package <pkg>'),
  );
  console.log(
    tr(
      'Write: lan config set <key> <value> --package <pkg>',
      '写入：lan config set <key> <value> --package <pkg>',
    ),
  );
}

/**
 * 一键覆盖：把全局配置下发到全部已安装命令子包。
 *
 * 只覆盖子包 manifest 声明过的键；未声明的键不动。
 */
function configApplyCommand(known, args) {
  const states = loadPackageStates();
  const globalConfig = loadGlobalPreferences();
  const explicit = {};
  let onlyPackage = null;

  // `lan config apply [--package <pkg>] [--set <dotted.key>=<value>]...`
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--package' || arg === '-p') {
      const raw = args[++index];
      onlyPackage = normalizePackageName(raw, known);
      if (!onlyPackage) {
        console.error(tr(`Unknown package: ${raw ?? ''}`, `未知子包：${raw ?? ''}`));
        process.exit(1);
      }
    } else if (arg === '--set' || arg === '-s') {
      const pair = args[++index] ?? '';
      const at = pair.indexOf('=');
      if (at <= 0) {
        console.error(tr('--set expects <key>=<value>', '--set 需要 <key>=<value> 格式'));
        process.exit(1);
      }
      const rawValue = pair.slice(at + 1);
      let value = rawValue;
      try {
        value = JSON.parse(rawValue);
      } catch {
        // 不是 JSON 就按字符串处理。
      }
      explicit[pair.slice(0, at)] = value;
    }
  }

  const sourceKeys = Object.keys({ ...globalConfig, ...explicit });
  if (sourceKeys.length === 0) {
    console.log(
      tr('No global config or --set values to apply.', '全局配置与 --set 均为空，无需覆盖。'),
    );
    return;
  }

  const applied = [];
  const skipped = [];
  for (const entry of known.commands) {
    if (entry.command === 'config') continue;
    if (onlyPackage && entry.command !== onlyPackage) continue;
    const cmdDir = resolveCommandPackageDir(entry.package);
    if (!cmdDir) continue;
    if (isPackageDisabled(entry.command, states)) continue;
    const schemaKeys = schemaKeyNames(readPackageConfigSchema(entry, cmdDir));
    const overrides = globalOverridesFor(globalConfig, schemaKeys, explicit);
    if (Object.keys(overrides).length === 0) {
      skipped.push(entry.command);
      continue;
    }
    const config = loadPackageConfig(entry.command);
    for (const [key, value] of Object.entries(overrides)) {
      setDottedValue(config, key, value);
    }
    savePackageConfig(entry.command, config);
    applied.push({ command: entry.command, keys: Object.keys(overrides) });
  }
  for (const item of applied) {
    console.log(
      tr(
        `  ${item.command.padEnd(10)} applied: ${item.keys.join(', ')}`,
        `  ${item.command.padEnd(10)} 已覆盖：${item.keys.join(', ')}`,
      ),
    );
  }
  if (applied.length === 0) {
    console.log(
      tr(
        `No subpackage declares an overridable key (source keys: ${sourceKeys.join(', ')}).`,
        `没有子包声明可覆盖的配置键（可用键：${sourceKeys.join(', ')}）。`,
      ),
    );
  } else {
    console.log('');
    console.log(
      tr(`Applied to ${applied.length} subpackage(s).`, `已覆盖 ${applied.length} 个子包。`),
    );
  }
  if (skipped.length > 0) {
    console.log(
      tr(
        `Skipped ${skipped.length} subpackage(s) without matching keys: ${skipped.join(', ')}`,
        `跳过 ${skipped.length} 个未声明相关配置键的子包：${skipped.join(', ')}`,
      ),
    );
  }
}

/** 全局用户偏好（~/.lania/preferences.json）。 */
function loadGlobalPreferences() {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) return {};
  try {
    const parsed = JSON.parse(
      fs.readFileSync(path.join(home, '.lania', 'preferences.json'), 'utf8'),
    );
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function packageCommand(known, args) {
  const [subcommand, ...rest] = args;

  if (!subcommand || ['--help', '-h', 'help'].includes(subcommand)) {
    console.log('lan package — 管理命令子包');
    console.log('');
    console.log('用法：');
    console.log('  lan package list                 列出全部子包及安装状态');
    console.log('  lan package install <cmd>        安装子包');
    console.log('  lan package uninstall <cmd>      卸载子包');
    console.log('  lan package update <cmd>         更新子包');
    console.log('  lan package enable <cmd>         启用子包');
    console.log('  lan package disable <cmd>        禁用子包（不卸载）');
    return;
  }

  switch (subcommand) {
    case 'list':
    case 'ls':
      printPackageList(known);
      return;

    case 'install':
    case 'add': {
      const command = rest.find((item) => !item.startsWith('-'));
      if (!command) {
        console.error('用法：lan package install <cmd>');
        process.exit(1);
      }
      const entry = requireCommandEntry(known, command);
      console.log(`[lan package] 安装 ${entry.package} …`);
      const code = runNpm(['install', entry.package]);
      if (code !== 0) process.exit(code);
      // 安装即视为启用：清掉 disabled 标记。
      const states = loadPackageStates();
      if (states[command]) {
        delete states[command].disabled;
        savePackageStates(states);
      }
      console.log(`[lan package] ${entry.package} 安装完成。`);
      return;
    }

    case 'uninstall':
    case 'remove':
    case 'rm': {
      const command = rest.find((item) => !item.startsWith('-'));
      if (!command) {
        console.error('用法：lan package uninstall <cmd>');
        process.exit(1);
      }
      const entry = requireCommandEntry(known, command);
      console.log(`[lan package] 卸载 ${entry.package} …`);
      const code = runNpm(['uninstall', entry.package]);
      if (code !== 0) process.exit(code);
      const states = loadPackageStates();
      delete states[command];
      savePackageStates(states);
      console.log(`[lan package] ${entry.package} 已卸载。`);
      return;
    }

    case 'update':
    case 'upgrade': {
      const command = rest.find((item) => !item.startsWith('-'));
      if (!command) {
        console.error('用法：lan package update <cmd>');
        process.exit(1);
      }
      const entry = requireCommandEntry(known, command);
      console.log(`[lan package] 更新 ${entry.package} …`);
      const code = runNpm(['update', entry.package]);
      if (code !== 0) process.exit(code);
      console.log(`[lan package] ${entry.package} 已更新。`);
      return;
    }

    case 'enable':
    case 'disable': {
      const command = rest.find((item) => !item.startsWith('-'));
      if (!command) {
        console.error(`用法：lan package ${subcommand} <cmd>`);
        process.exit(1);
      }
      requireCommandEntry(known, command);
      const states = loadPackageStates();
      states[command] = { ...(states[command] ?? {}), disabled: subcommand === 'disable' };
      savePackageStates(states);
      console.log(`[lan package] ${command} 已${subcommand === 'disable' ? '禁用' : '启用'}。`);
      return;
    }

    default:
      console.error(`未知子命令 "lan package ${subcommand}"。`);
      console.error('用法：lan package list | install | uninstall | update | enable | disable');
      process.exit(1);
  }
}

// ── 主入口 ────────────────────────────────────────────────────────────

function main() {
  const suffix = platformSuffix();
  if (!suffix) {
    console.error(
      `Unsupported platform: ${process.platform} ${process.arch}. ` +
        'Supported packaged platforms: darwin arm64, linux x64, win32 x64.',
    );
    process.exit(1);
  }

  // 总包（@lania-cli-v2/cli）不再是「静态链接全部命令的单体」：
  // 它是**管理面** —— 提供 `lan package`（子包增删改查）与 `lan config`
  // （全局/项目/跨子包配置），命令能力完全由各命令子包提供。
  // 因此 launcher **总是走调度模式**：命令是否可用只取决于对应子包是否安装。
  //
  // 兼容：`LANIA_MONOLITH_BINARY` 仍可显式指向旧单体二进制（开发调试用）。
  const explicitMonolith = process.env.LANIA_MONOLITH_BINARY;
  if (explicitMonolith && fs.existsSync(explicitMonolith)) {
    let cliDir = null;
    try {
      cliDir = path.dirname(require.resolve('@lania-cli-v2/cli/package.json'));
    } catch {
      cliDir = null;
    }
    execDirect(explicitMonolith, monolithEnv(cliDir), argv);
    return;
  }

  dispatch(loadKnownCommands());
}

main();
