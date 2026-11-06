/**
 * `tools.pm`：包管理器探测、命令规划和实际执行的统一入口。
 *
 * 这一层把三类能力放在同一个命名空间里：
 * - 只读探测：当前项目更像哪种生态的哪种包管理器
 * - 纯规划：生成 install/remove/run/publish 等命令快照
 * - 带副作用执行：真正通过 exec/host 调起包管理器
 *
 * 规划与 spec 以 Rust 侧 `lania-pm` 为唯一权威；Node 侧只保留
 * `detectFromFiles/binary/lockfile/lockfileStrategy` 这类同步静态查询。
 */
import { hostCall } from '../core/host-rpc.js';
import { asRecord } from '../core/runtime.js';
import type { ExecRunResult } from './exec.js';
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

type PackageManagerOptions = {
  manager?: string;
  cwd?: string;
  language?: string;
};

type CommandOptions = PackageManagerOptions;

type CommandDto = { program: string; args: string[] };

// 公开接口同时覆盖“探测 / 规划 / 执行”三个层次，保持 v2.3 对 schema 作者承诺的稳定 API 面。
export interface PackageManagerTools {
  detectFromFiles: (files: string[]) => string;
  detect: (cwd?: string, options?: { language?: string }) => Promise<string>;
  supportedManagers: () => Promise<string[]>;
  spec: (
    managerOrCwd?: string,
    options?: { language?: string },
  ) => Promise<Record<string, unknown>>;
  binary: (manager: string) => string;
  lockfile: (manager: string) => string;
  lockfileStrategy: (manager: string) => string;
  loadPackageJsonSnapshot: (cwd?: string) => Promise<Record<string, unknown>>;
  scriptExists: (script: string, cwd?: string) => Promise<boolean>;
  requireScript: (script: string, cwd?: string) => Promise<void>;
  init: (options?: PackageManagerOptions) => Promise<ExecRunResult>;
  installAll: (options?: PackageManagerOptions) => Promise<ExecRunResult>;
  install: (
    packages: string[],
    options?: PackageManagerOptions & { dev?: boolean },
  ) => Promise<ExecRunResult>;
  remove: (packages: string[], options?: PackageManagerOptions) => Promise<ExecRunResult>;
  update: (packages: string[], options?: PackageManagerOptions) => Promise<ExecRunResult>;
  run: (
    script: string,
    extraArgs?: string[],
    options?: PackageManagerOptions & { checked?: boolean },
  ) => Promise<ExecRunResult>;
  publish: (options?: PackageManagerOptions & { tag?: string }) => Promise<ExecRunResult>;
  command: {
    init: (manager?: string) => Promise<CommandDto>;
    installAll: (manager?: string) => Promise<CommandDto>;
    install: (
      packages: string[],
      options?: PackageManagerOptions & { dev?: boolean },
    ) => Promise<CommandDto>;
    remove: (packages: string[], options?: PackageManagerOptions) => Promise<CommandDto>;
    update: (packages: string[], options?: PackageManagerOptions) => Promise<CommandDto>;
    runScript: (
      script: string,
      options?: PackageManagerOptions & { args?: string[]; checked?: boolean },
    ) => Promise<CommandDto>;
    runScriptChecked: (
      script: string,
      options?: PackageManagerOptions & { args?: string[] },
    ) => Promise<CommandDto>;
    publish: (options?: PackageManagerOptions & { tag?: string }) => Promise<CommandDto>;
    addDependencyCommands: (
      options?: PackageManagerOptions & {
        dependencies?: string[];
        devDependencies?: string[];
      },
    ) => Promise<CommandDto[]>;
  };
}

const SUPPORTED_PM = [
  'npm',
  'pnpm',
  'yarn',
  'bun',
  'pip',
  'uv',
  'poetry',
  'cargo',
  'go',
  'pub',
  'bundler',
  'composer',
] as const;
type SupportedPm = (typeof SUPPORTED_PM)[number];

const BINARY: Record<SupportedPm, string> = {
  npm: 'npm',
  pnpm: 'pnpm',
  yarn: 'yarn',
  bun: 'bun',
  pip: 'python',
  uv: 'uv',
  poetry: 'poetry',
  cargo: 'cargo',
  go: 'go',
  pub: 'dart',
  bundler: 'bundle',
  composer: 'composer',
};

const LOCKFILE: Record<SupportedPm, string> = {
  npm: 'package-lock.json',
  pnpm: 'pnpm-lock.yaml',
  yarn: 'yarn.lock',
  bun: 'bun.lockb',
  pip: 'requirements.txt',
  uv: 'uv.lock',
  poetry: 'poetry.lock',
  cargo: 'Cargo.lock',
  go: 'go.sum',
  pub: 'pubspec.lock',
  bundler: 'Gemfile.lock',
  composer: 'composer.lock',
};

const isSupportedPm = (value: string): value is SupportedPm =>
  (SUPPORTED_PM as readonly string[]).includes(value);

const detectFromFiles = (files: string[]): SupportedPm => {
  const has = (name: string) => files.includes(name);
  if (has('pnpm-lock.yaml')) return 'pnpm';
  if (has('yarn.lock')) return 'yarn';
  if (has('bun.lockb') || has('bun.lock')) return 'bun';
  if (has('uv.lock')) return 'uv';
  if (has('poetry.lock')) return 'poetry';
  if (has('Cargo.lock')) return 'cargo';
  if (has('go.sum')) return 'go';
  if (has('pubspec.lock')) return 'pub';
  if (has('Gemfile.lock')) return 'bundler';
  if (has('composer.lock')) return 'composer';
  if (has('package-lock.json')) return 'npm';
  if (has('Cargo.toml')) return 'cargo';
  if (has('go.mod')) return 'go';
  if (has('pubspec.yaml')) return 'pub';
  if (has('Gemfile')) return 'bundler';
  if (has('composer.json')) return 'composer';
  if (has('package.json')) return 'npm';
  if (has('pyproject.toml') || has('requirements.txt')) return 'pip';
  return 'npm';
};

export function createPackageManagerTools(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
): PackageManagerTools {
  // 只读查询和 host-backed 命令规划共用同一个 invoker，
  // 这样策略校验与审计元数据能和其它 schema tools 保持一致。
  const host = createHostInvoker(base);

  const managerParams = (options?: PackageManagerOptions): Record<string, unknown> => ({
    cwd: options?.cwd ?? base.cwd,
    ...(options?.manager ? { manager: options.manager } : {}),
    ...(options?.language ? { language: options.language } : {}),
  });

  // `binary()` 是所有对外 manager 输入的最后一道归一化关口。
  // 未知值统一降级到 `npm`，避免 unsupported-manager 分支在各处扩散。
  const binary = (manager: string): string => (isSupportedPm(manager) ? BINARY[manager] : 'npm');
  const lockfile = (manager: string): string =>
    isSupportedPm(manager) ? LOCKFILE[manager] : LOCKFILE.npm;

  const commandInit = async (options?: CommandOptions): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.init', managerParams(options));

  const commandInstallAll = async (options?: CommandOptions): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.installAll', managerParams(options));

  const commandInstall = async (
    packages: string[],
    options?: CommandOptions & { dev?: boolean },
  ): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.install', {
      ...managerParams(options),
      packages,
      dev: options?.dev ?? false,
    });

  const commandRemove = async (packages: string[], options?: CommandOptions): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.remove', {
      ...managerParams(options),
      packages,
    });

  const commandUpdate = async (packages: string[], options?: CommandOptions): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.update', {
      ...managerParams(options),
      packages,
    });

  const commandRunScript = async (
    script: string,
    options?: CommandOptions & { args?: string[]; checked?: boolean },
  ): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.runScript', {
      ...managerParams(options),
      script,
      args: options?.args ?? [],
      checked: options?.checked ?? false,
    });

  const commandPublish = async (options?: CommandOptions & { tag?: string }): Promise<CommandDto> =>
    host.call<CommandDto>('host.pm.command.publish', {
      ...managerParams(options),
      tag: options?.tag,
    });

  const commandAddDependencyCommands = async (
    options?: CommandOptions & { dependencies?: string[]; devDependencies?: string[] },
  ): Promise<CommandDto[]> => {
    const result = await host.call<{ commands?: CommandDto[] }>(
      'host.pm.command.addDependencyCommands',
      {
        ...managerParams(options),
        dependencies: options?.dependencies ?? [],
        devDependencies: options?.devDependencies ?? [],
      },
    );
    return Array.isArray(result.commands) ? result.commands : [];
  };

  // 高层执行统一走 host exec，这样包管理器工作流会继承与 `tools.exec` 一致的
  // timeout、policy 和 audit 边界。
  const runExecCommand = async (
    command: { program: string; args: string[] },
    options?: { cwd?: string; checked?: boolean },
  ): Promise<ExecRunResult> => {
    // 这里有意委托给 host exec，而不是直接起进程，
    // 让超时、审计和策略行为与其它 `ctx.tools` 执行面保持一致。
    const exchange = await hostCall<ExecRunResult>(
      options?.checked ? 'host.exec.runChecked' : 'host.exec.run',
      {
        program: command.program,
        args: command.args,
        cwd: options?.cwd ?? base.cwd,
        env: {},
        killProcessTree: false,
        useShell: false,
      },
    );
    return exchange.result;
  };

  // 对外 API 有意混合三层能力：
  // - 只读 manager/package.json helper
  // - 纯 `command.*` 规划器
  // - 先规划再通过 host exec 执行的高层方法
  // 这样既能保持文档承诺的 API 面，也不需要改变 `createSchemaTools()` 作为唯一装配入口的角色。
  const tools: PackageManagerTools = {
    detectFromFiles,
    detect: async (cwd, options) => {
      await policy.assertPmAllowed('detect');
      const result = await host.call<{ manager: string }>('host.pm.detect', {
        cwd: cwd ?? base.cwd,
        ...(options?.language ? { language: options.language } : {}),
      });
      return result.manager;
    },
    supportedManagers: async () => {
      await policy.assertPmAllowed('supportedManagers');
      const result = await host.call<{ managers: string[] }>('host.pm.supportedManagers');
      return Array.isArray(result.managers) ? result.managers : [];
    },
    spec: async (managerOrCwd, options) => {
      await policy.assertPmAllowed('spec');
      // `spec()` 故意支持两种调用方式：
      // - 传显式 manager：直接让 Rust 返回该 manager 的 spec
      // - 传 cwd/undefined：让 host 探测当前项目使用的 manager
      if (managerOrCwd && isSupportedPm(managerOrCwd)) {
        return asRecord(
          await host.call<Record<string, unknown>>('host.pm.spec', {
            manager: managerOrCwd,
          }),
        );
      }
      const result = await host.call<Record<string, unknown>>('host.pm.spec', {
        cwd: managerOrCwd ?? base.cwd,
        ...(options?.language ? { language: options.language } : {}),
      });
      return asRecord(result);
    },
    binary,
    lockfile,
    lockfileStrategy: (manager) => `${manager} uses ${lockfile(manager)}`,
    loadPackageJsonSnapshot: async (cwd) => {
      await policy.assertPmAllowed('loadPackageJsonSnapshot');
      const result = await host.call<Record<string, unknown>>('host.pm.loadPackageJsonSnapshot', {
        cwd: cwd ?? base.cwd,
      });
      return asRecord(result);
    },
    scriptExists: async (script, cwd) => {
      await policy.assertPmAllowed('scriptExists');
      const result = await host.call<{ exists: boolean }>('host.pm.scriptExists', {
        cwd: cwd ?? base.cwd,
        script,
      });
      return Boolean(result.exists);
    },
    requireScript: async (script, cwd) => {
      await policy.assertPmAllowed('requireScript');
      const snapshot = await tools.loadPackageJsonSnapshot(cwd);
      const exists = Boolean(snapshot.exists);
      const scripts = asRecord(snapshot.scripts);
      if (!exists) {
        throw new Error(`package.json not found in ${cwd ?? base.cwd}`);
      }
      if (script in scripts) {
        return;
      }
      const available = Object.keys(scripts).join(', ');
      throw new Error(
        `script \`${script}\` not found in package.json (available: ${available || 'none'})`,
      );
    },
    init: async (options) => {
      await policy.assertPmAllowed('init');
      return runExecCommand(await commandInit(options), { cwd: options?.cwd });
    },
    installAll: async (options) => {
      await policy.assertPmAllowed('installAll');
      return runExecCommand(await commandInstallAll(options), { cwd: options?.cwd });
    },
    install: async (packages, options) => {
      await policy.assertPmAllowed('install');
      return runExecCommand(await commandInstall(packages, options), { cwd: options?.cwd });
    },
    remove: async (packages, options) => {
      await policy.assertPmAllowed('remove');
      return runExecCommand(await commandRemove(packages, options), { cwd: options?.cwd });
    },
    update: async (packages, options) => {
      await policy.assertPmAllowed('update');
      return runExecCommand(await commandUpdate(packages, options), { cwd: options?.cwd });
    },
    run: async (script, extraArgs, options) => {
      await policy.assertPmAllowed('run');
      if (options?.checked) {
        await tools.requireScript(script, options?.cwd);
      }
      return runExecCommand(
        await commandRunScript(script, {
          manager: options?.manager,
          cwd: options?.cwd,
          language: options?.language,
          args: extraArgs ?? [],
          checked: options?.checked,
        }),
        { cwd: options?.cwd, checked: options?.checked },
      );
    },
    publish: async (options) => {
      await policy.assertPmAllowed('publish');
      return runExecCommand(await commandPublish(options), { cwd: options?.cwd });
    },
    command: {
      init: async (manager) => {
        await policy.assertPmAllowed('command.init');
        return commandInit({ manager });
      },
      installAll: async (manager) => {
        await policy.assertPmAllowed('command.installAll');
        return commandInstallAll({ manager });
      },
      install: async (packages, options) => {
        await policy.assertPmAllowed('command.install');
        return commandInstall(packages, options);
      },
      remove: async (packages, options) => {
        await policy.assertPmAllowed('command.remove');
        return commandRemove(packages, options);
      },
      update: async (packages, options) => {
        await policy.assertPmAllowed('command.update');
        return commandUpdate(packages, options);
      },
      runScript: async (script, options) => {
        await policy.assertPmAllowed('command.runScript');
        if (options?.checked) {
          await tools.requireScript(script, options?.cwd);
        }
        return commandRunScript(script, options);
      },
      runScriptChecked: async (script, options) => {
        await policy.assertPmAllowed('command.runScriptChecked');
        await tools.requireScript(script, options?.cwd);
        return commandRunScript(script, { ...options, checked: true });
      },
      publish: async (options) => {
        await policy.assertPmAllowed('command.publish');
        return commandPublish(options);
      },
      addDependencyCommands: async (options) => {
        await policy.assertPmAllowed('command.addDependencyCommands');
        return commandAddDependencyCommands(options);
      },
    },
  };

  return tools;
}
