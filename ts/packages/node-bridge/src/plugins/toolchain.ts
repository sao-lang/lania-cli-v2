/**
 * 运行时知识的**唯一来源**。
 *
 * 设计目标（见 `docs/design/multi-runtime-bridge.zh-CN.md` §5）：
 * 同一份「特征文件 → 运行时」「运行时 → 包管理器 / build 命令 / lint 工具」的
 * 知识过去散落在 4 个位置且互相漂移。现在全部收拢到这里，并且：
 *
 * - `toolchain.describe` bridge 方法把整张表暴露给 Rust（Rust 不再自己维护副本）。
 * - `compiler-shared.ts` 的运行时探测、`runtime-adapters/commands.ts` 的默认命令、
 *   `lint/` 的 adaptor → 运行时映射都改为读这里。
 *
 * 约束：
 * - 本文件**不执行任何 IO**，保持纯数据 + 纯函数，方便单测与跨进程复用。
 * - 新增运行时只需要改这里一处。
 */

/** 本文覆盖的七个运行时。 */
export const RUNTIME_IDS = ['node', 'bun', 'deno', 'python', 'go', 'rust', 'dart'] as const;
export type RuntimeId = (typeof RUNTIME_IDS)[number];

export interface LintToolSpec {
  /** adaptor id，与 `lint.run` 的 `linters` 参数一致。 */
  id: string;
  binary: string;
  args: string[];
  /** bridge 进程内可 `import` 该工具（无需 spawn）。 */
  inProcess: boolean;
  /** 执行器覆盖（bun 用 `bunx`；deno 用内置）。 */
  runner?: string;
}

export interface RuntimeToolchain {
  id: RuntimeId;
  /** 是否为 JS 簇（node / bun / deno 共用同一个 bridge 进程）。 */
  jsCluster: boolean;
  /** 特征文件：任一存在即判定；**按数组顺序做优先级判定**。 */
  featureFiles: string[];
  /** 额外特征文件（同优先级，按存在即命中处理，例如锁文件）。 */
  lockFiles: string[];
  packageManagers: string[];
  /** 空数组表示不支持。 */
  buildCommand: string[];
  devCommand: string[];
  lintTools: LintToolSpec[];
  /** 供 UI/模板展示的别名。 */
  displayName: string;
}

/**
 * 判定优先级（必须固定，否则同一项目两次运行可能判定不同）：
 * deno.json > bun.lock(b) > package.json > pyproject.toml > go.mod > Cargo.toml > pubspec.yaml
 *
 * 数组顺序即优先级顺序。
 */
export const RUNTIME_TOOLCHAINS: readonly RuntimeToolchain[] = [
  {
    id: 'deno',
    jsCluster: true,
    featureFiles: ['deno.json', 'deno.jsonc'],
    lockFiles: ['deno.lock'],
    packageManagers: ['deno'],
    buildCommand: ['deno', 'task', 'build'],
    devCommand: ['deno', 'task', 'dev'],
    lintTools: [
      // deno lint / deno fmt 是 Deno 内置的 Rust 实现，没有 JS 模块形态，只能 spawn。
      { id: 'deno-lint', binary: 'deno', args: ['lint'], inProcess: false, runner: 'deno' },
      {
        id: 'deno-fmt',
        binary: 'deno',
        args: ['fmt', '--check'],
        inProcess: false,
        runner: 'deno',
      },
    ],
    displayName: 'Deno',
  },
  {
    id: 'bun',
    jsCluster: true,
    featureFiles: [],
    lockFiles: ['bun.lockb', 'bun.lock'],
    packageManagers: ['bun'],
    buildCommand: ['bun', 'build'],
    devCommand: ['bun', 'run', '--watch'],
    lintTools: [
      // Bun 有 Node 兼容层，可直接执行 eslint 二进制（通过 bunx，而非 node_modules/.bin）。
      { id: 'eslint', binary: 'eslint', args: [], inProcess: true, runner: 'bunx' },
      { id: 'oxlint', binary: 'oxlint', args: [], inProcess: false, runner: 'bunx' },
    ],
    displayName: 'Bun',
  },
  {
    id: 'node',
    jsCluster: true,
    featureFiles: ['package.json'],
    lockFiles: ['package-lock.json', 'pnpm-lock.yaml', 'yarn.lock'],
    packageManagers: ['npm', 'pnpm', 'yarn'],
    buildCommand: [],
    devCommand: [],
    lintTools: [
      { id: 'eslint', binary: 'eslint', args: [], inProcess: true },
      { id: 'stylelint', binary: 'stylelint', args: [], inProcess: true },
      { id: 'textlint', binary: 'textlint', args: [], inProcess: true },
      { id: 'oxlint', binary: 'oxlint', args: [], inProcess: false },
      { id: 'oxfmt', binary: 'oxfmt', args: [], inProcess: false },
      { id: 'prettier', binary: 'prettier', args: [], inProcess: true },
    ],
    displayName: 'Node.js',
  },
  {
    id: 'python',
    jsCluster: false,
    featureFiles: ['pyproject.toml', 'setup.py'],
    lockFiles: ['poetry.lock', 'uv.lock', 'Pipfile.lock'],
    packageManagers: ['pip', 'uv', 'poetry'],
    buildCommand: ['python', '-m', 'build'],
    devCommand: [],
    lintTools: [
      { id: 'mypy', binary: 'mypy', args: ['.'], inProcess: false },
      { id: 'ruff', binary: 'ruff', args: ['check'], inProcess: false },
      { id: 'black', binary: 'black', args: ['--check', '.'], inProcess: false },
      { id: 'flake8', binary: 'flake8', args: ['.'], inProcess: false },
    ],
    displayName: 'Python',
  },
  {
    id: 'go',
    jsCluster: false,
    featureFiles: ['go.mod'],
    lockFiles: ['go.sum'],
    packageManagers: ['go'],
    buildCommand: ['go', 'build', '-o', '{output}', '.'],
    devCommand: ['go', 'run', '.'],
    lintTools: [
      { id: 'golangci-lint', binary: 'golangci-lint', args: ['run', './...'], inProcess: false },
      { id: 'go-vet', binary: 'go', args: ['vet', './...'], inProcess: false },
    ],
    displayName: 'Go',
  },
  {
    id: 'rust',
    jsCluster: false,
    featureFiles: ['Cargo.toml'],
    lockFiles: ['Cargo.lock'],
    packageManagers: ['cargo'],
    buildCommand: ['cargo', 'build', '--release'],
    devCommand: ['cargo', 'run'],
    lintTools: [{ id: 'clippy', binary: 'cargo', args: ['clippy'], inProcess: false }],
    displayName: 'Rust',
  },
  {
    id: 'dart',
    jsCluster: false,
    featureFiles: ['pubspec.yaml'],
    lockFiles: ['pubspec.lock'],
    packageManagers: ['pub'],
    buildCommand: ['dart', 'compile', 'exe', 'bin/main.dart'],
    devCommand: ['dart', 'run'],
    lintTools: [{ id: 'dart-analyze', binary: 'dart', args: ['analyze'], inProcess: false }],
    displayName: 'Dart',
  },
];

export function isRuntimeId(value: unknown): value is RuntimeId {
  return typeof value === 'string' && (RUNTIME_IDS as readonly string[]).includes(value);
}

export function toolchainFor(id: string): RuntimeToolchain | null {
  return RUNTIME_TOOLCHAINS.find((entry) => entry.id === id) ?? null;
}

/** 特征文件 → 运行时（按运行时表顺序展开，不含优先级语义）。 */
export function featureFileOrder(): Array<{ file: string; runtime: RuntimeId }> {
  const out: Array<{ file: string; runtime: RuntimeId }> = [];
  for (const entry of RUNTIME_TOOLCHAINS) {
    for (const file of entry.featureFiles) {
      out.push({ file, runtime: entry.id });
    }
  }
  return out;
}

/**
 * 特征文件（含锁文件）→ 运行时，**按判定优先级排列**。
 *
 * 优先级（§5.1，必须固定）：
 * deno.json / deno.jsonc > bun.lock(b) > pyproject.toml / go.mod / Cargo.toml / pubspec.yaml
 * > package.json（node 兜底）
 *
 * 两条容易踩错的约束：
 * 1. `bun.lockb` 必须排在 `package.json` **之前** —— 否则 bun 项目会被判成 node。
 * 2. 非 JS 强信号（`go.mod` / `Cargo.toml` / `pyproject.toml` / `pubspec.yaml`）也必须排在
 *    `package.json` 之前 —— 否则「Go 项目里带一个 package.json 做前端资源」会被判成 web。
 */
export const DETECTION_PRIORITY: ReadonlyArray<{ file: string; runtime: RuntimeId }> = [
  { file: 'deno.json', runtime: 'deno' },
  { file: 'deno.jsonc', runtime: 'deno' },
  { file: 'bun.lockb', runtime: 'bun' },
  { file: 'bun.lock', runtime: 'bun' },
  { file: 'pyproject.toml', runtime: 'python' },
  { file: 'go.mod', runtime: 'go' },
  { file: 'Cargo.toml', runtime: 'rust' },
  { file: 'pubspec.yaml', runtime: 'dart' },
  // node 兜底放最后：`package.json` 几乎总是和别的特征文件共存。
  { file: 'package.json', runtime: 'node' },
];

export function detectionOrder(): ReadonlyArray<{ file: string; runtime: RuntimeId }> {
  return DETECTION_PRIORITY;
}

/** 该运行时登记的 lint adaptor id。 */
export function lintToolsFor(id: RuntimeId): LintToolSpec[] {
  return toolchainFor(id)?.lintTools ?? [];
}

/** `toolchain.describe` 的返回体。 */
export function describeToolchains() {
  return {
    runtimes: RUNTIME_TOOLCHAINS.map((entry) => ({
      id: entry.id,
      jsCluster: entry.jsCluster,
      featureFiles: [...entry.featureFiles, ...entry.lockFiles],
      packageManagers: entry.packageManagers,
      buildCommand: entry.buildCommand,
      devCommand: entry.devCommand,
      displayName: entry.displayName,
      lintTools: entry.lintTools.map((tool) => ({ ...tool })),
    })),
    /** 判定优先级（文件 → 运行时），供 Rust / 文档复用。 */
    detectionOrder: DETECTION_PRIORITY.map((item) => ({
      file: item.file,
      runtime: item.runtime,
    })),
  };
}

export type ToolchainDescription = ReturnType<typeof describeToolchains>;
