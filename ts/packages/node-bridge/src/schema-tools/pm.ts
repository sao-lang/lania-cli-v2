/**
 * `tools.pm`：包管理器探测、命令规划和实际执行的统一入口。
 *
 * 这一层把三类能力放在同一个命名空间里：
 * - 只读探测：当前项目更像 npm / pnpm / yarn / bun 中的哪一种
 * - 纯规划：生成 install/remove/run/publish 等命令快照
 * - 带副作用执行：真正通过 exec/host 调起包管理器
 *
 * 设计上尽量让 schema 作者少关心“当前项目到底用哪个 manager”，
 * 只在需要时显式指定，其余场景都走自动探测与稳定兜底。
 */
import { hostCall } from '../core/host-rpc.js';
import { asRecord } from '../core/runtime.js';
import type { ExecRunResult } from './exec.js';
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

// 公开接口同时覆盖“探测 / 规划 / 执行”三个层次，保持 v2.3 对 schema 作者承诺的稳定 API 面。
export interface PackageManagerTools {
  detectFromFiles: (files: string[]) => string;
  detect: (cwd?: string) => Promise<string>;
  supportedManagers: () => Promise<string[]>;
  spec: (managerOrCwd?: string) => Promise<Record<string, unknown>>;
  binary: (manager: string) => string;
  lockfile: (manager: string) => string;
  lockfileStrategy: (manager: string) => string;
  loadPackageJsonSnapshot: (cwd?: string) => Promise<Record<string, unknown>>;
  scriptExists: (script: string, cwd?: string) => Promise<boolean>;
  requireScript: (script: string, cwd?: string) => Promise<void>;
  init: (options?: { manager?: string; cwd?: string }) => Promise<ExecRunResult>;
  installAll: (options?: { manager?: string; cwd?: string }) => Promise<ExecRunResult>;
  install: (
    packages: string[],
    options?: { manager?: string; dev?: boolean; cwd?: string },
  ) => Promise<ExecRunResult>;
  remove: (
    packages: string[],
    options?: { manager?: string; cwd?: string },
  ) => Promise<ExecRunResult>;
  update: (
    packages: string[],
    options?: { manager?: string; cwd?: string },
  ) => Promise<ExecRunResult>;
  run: (
    script: string,
    extraArgs?: string[],
    options?: { manager?: string; cwd?: string; checked?: boolean },
  ) => Promise<ExecRunResult>;
  publish: (options?: { manager?: string; tag?: string; cwd?: string }) => Promise<ExecRunResult>;
  command: {
    init: (manager?: string) => Promise<{ program: string; args: string[] }>;
    installAll: (manager?: string) => Promise<{ program: string; args: string[] }>;
    install: (
      packages: string[],
      options?: { manager?: string; cwd?: string; dev?: boolean },
    ) => Promise<{ program: string; args: string[] }>;
    remove: (
      packages: string[],
      options?: { manager?: string; cwd?: string },
    ) => Promise<{ program: string; args: string[] }>;
    update: (
      packages: string[],
      options?: { manager?: string; cwd?: string },
    ) => Promise<{ program: string; args: string[] }>;
    runScript: (
      script: string,
      options?: { manager?: string; cwd?: string; args?: string[]; checked?: boolean },
    ) => Promise<{ program: string; args: string[] }>;
    runScriptChecked: (
      script: string,
      options?: { manager?: string; cwd?: string; args?: string[] },
    ) => Promise<{ program: string; args: string[] }>;
    publish: (options?: {
      manager?: string;
      cwd?: string;
      tag?: string;
    }) => Promise<{ program: string; args: string[] }>;
    addDependencyCommands: (options?: {
      manager?: string;
      dependencies?: string[];
      devDependencies?: string[];
      cwd?: string;
    }) => Promise<Array<{ program: string; args: string[] }>>;
  };
}

export function createPackageManagerTools(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
): PackageManagerTools {
  // 只读查询和 host-backed 命令规划共用同一个 invoker，
  // 这样策略校验与审计元数据能和其它 schema tools 保持一致。
  const host = createHostInvoker(base);
  const SUPPORTED_PM = ['npm', 'pnpm', 'yarn', 'bun'] as const;
  type SupportedPm = (typeof SUPPORTED_PM)[number];

  const isSupportedPm = (value: string): value is SupportedPm =>
    (SUPPORTED_PM as readonly string[]).includes(value);

  const detectFromFiles = (files: string[]): SupportedPm => {
    if (files.includes('pnpm-lock.yaml')) return 'pnpm';
    if (files.includes('yarn.lock')) return 'yarn';
    if (files.includes('bun.lockb') || files.includes('bun.lock')) return 'bun';
    return 'npm';
  };

  // `binary()` 是所有对外 manager 输入的最后一道归一化关口。
  // 未知值统一降级到 `npm`，避免 unsupported-manager 分支在各处扩散。
  const binary = (manager: string): string => (isSupportedPm(manager) ? manager : 'npm');

  // 这些静态元数据完全由本地决定，放在 Node 侧维护更便宜，也更方便随版本演进。
  const lockfile = (manager: string): string => {
    switch (binary(manager)) {
      case 'pnpm':
        return 'pnpm-lock.yaml';
      case 'yarn':
        return 'yarn.lock';
      case 'bun':
        return 'bun.lockb';
      case 'npm':
      default:
        return 'package-lock.json';
    }
  };

  const buildSpec = (manager: SupportedPm): Record<string, unknown> => {
    switch (manager) {
      case 'npm':
        return {
          manager,
          binary: 'npm',
          init_subcommand: 'init',
          add_subcommand: 'install',
          install_subcommand: 'install',
          remove_subcommand: 'uninstall',
          update_subcommand: 'update',
          run_subcommand: 'run',
          save_flag: '--save',
          save_dev_flag: '--save-dev',
          silent_flag: '--silent',
          strict_peer_flag: '--legacy-peer-deps',
          init_flag: '-y',
          run_separator: '--',
          lockfile: 'package-lock.json',
        };
      case 'pnpm':
        return {
          manager,
          binary: 'pnpm',
          init_subcommand: 'init',
          add_subcommand: 'install',
          install_subcommand: 'install',
          remove_subcommand: 'remove',
          update_subcommand: 'update',
          run_subcommand: 'run',
          save_flag: '--save',
          save_dev_flag: '--save-dev',
          silent_flag: '--reporter=silent',
          strict_peer_flag: '--strict-peer-dependencies=false',
          init_flag: null,
          run_separator: '--',
          lockfile: 'pnpm-lock.yaml',
        };
      case 'yarn':
        return {
          manager,
          binary: 'yarn',
          init_subcommand: 'init',
          add_subcommand: 'add',
          install_subcommand: 'install',
          remove_subcommand: 'remove',
          update_subcommand: 'upgrade',
          run_subcommand: 'run',
          save_flag: null,
          save_dev_flag: '--dev',
          silent_flag: '--silent',
          strict_peer_flag: null,
          init_flag: '-y',
          run_separator: null,
          lockfile: 'yarn.lock',
        };
      case 'bun':
      default:
        return {
          manager,
          binary: 'bun',
          init_subcommand: 'init',
          add_subcommand: 'add',
          install_subcommand: 'install',
          remove_subcommand: 'remove',
          update_subcommand: 'update',
          run_subcommand: 'run',
          save_flag: null,
          save_dev_flag: '--dev',
          silent_flag: '--silent',
          strict_peer_flag: null,
          init_flag: '-y',
          run_separator: null,
          lockfile: 'bun.lockb',
        };
    }
  };

  // 规划器既接受显式 manager，也接受只给 cwd 然后自动探测。
  // 这样公共 helper 用起来更顺手，不需要在每个命令构造器里重复写探测逻辑。
  const resolveManager = async (manager?: string, cwd?: string): Promise<SupportedPm> => {
    if (manager && isSupportedPm(manager)) {
      return manager;
    }
    const detected = await tools.detect(cwd);
    return isSupportedPm(detected) ? detected : 'npm';
  };

  // 命令 DTO 故意保持很小，只保留 exec 真正需要的 program/args。
  const buildCommand = (program: string, args: string[]) => ({ program, args });

  // 下方 planning helper 都是纯函数式的：只产生命令快照，不直接执行。
  // 真正的副作用统一留给更外层 high-level API。
  const initCommand = async (manager?: string, cwd?: string) => {
    const spec = buildSpec(await resolveManager(manager, cwd));
    const args = [String(spec.init_subcommand)];
    if (typeof spec.init_flag === 'string' && spec.init_flag) {
      args.push(spec.init_flag);
    }
    return buildCommand(String(spec.binary), args);
  };

  const installAllCommand = async (manager?: string, cwd?: string) => {
    const spec = buildSpec(await resolveManager(manager, cwd));
    const args = [String(spec.install_subcommand)];
    if (typeof spec.strict_peer_flag === 'string' && spec.strict_peer_flag) {
      args.push(spec.strict_peer_flag);
    }
    return buildCommand(String(spec.binary), args);
  };

  const installCommand = async (
    packages: string[],
    options?: { manager?: string; cwd?: string; dev?: boolean },
  ) => {
    const resolved = await resolveManager(options?.manager, options?.cwd);
    const spec = buildSpec(resolved);
    const args = [String(spec.add_subcommand)];
    // peer dependency 放宽策略放在 planner 内部统一处理，
    // 这样 `install`、`installAll` 和依赖引导都能继承同一套默认行为。
    if (typeof spec.strict_peer_flag === 'string' && spec.strict_peer_flag) {
      args.push(spec.strict_peer_flag);
    }
    if (options?.dev) {
      args.push(String(spec.save_dev_flag));
    } else if (resolved === 'npm' && typeof spec.save_flag === 'string' && spec.save_flag) {
      args.push(spec.save_flag);
    }
    args.push(...packages);
    return buildCommand(String(spec.binary), args);
  };

  const removeCommand = async (
    packages: string[],
    options?: { manager?: string; cwd?: string },
  ) => {
    const spec = buildSpec(await resolveManager(options?.manager, options?.cwd));
    return buildCommand(String(spec.binary), [String(spec.remove_subcommand), ...packages]);
  };

  const updateCommand = async (
    packages: string[],
    options?: { manager?: string; cwd?: string },
  ) => {
    const spec = buildSpec(await resolveManager(options?.manager, options?.cwd));
    return buildCommand(String(spec.binary), [String(spec.update_subcommand), ...packages]);
  };

  const runScriptCommand = async (
    script: string,
    options?: { manager?: string; cwd?: string; args?: string[] },
  ) => {
    const spec = buildSpec(await resolveManager(options?.manager, options?.cwd));
    const extraArgs = options?.args ?? [];
    const args = [String(spec.run_subcommand), script];
    // 只有部分包管理器需要在用户参数前显式插入分隔符；
    // 这个差异收口在这里，调用方就不必自己写 manager 分支。
    if (extraArgs.length > 0 && typeof spec.run_separator === 'string' && spec.run_separator) {
      args.push(spec.run_separator);
    }
    args.push(...extraArgs);
    return buildCommand(String(spec.binary), args);
  };

  const publishCommand = async (options?: { manager?: string; cwd?: string; tag?: string }) => {
    const resolved = await resolveManager(options?.manager, options?.cwd);
    const args = ['publish'];
    if (options?.tag) {
      args.push('--tag', options.tag);
    }
    return buildCommand(resolved, args);
  };

  const addDependencyCommands = async (options?: {
    manager?: string;
    dependencies?: string[];
    devDependencies?: string[];
    cwd?: string;
  }) => {
    const commands: Array<{ program: string; args: string[] }> = [];
    // 保持声明顺序：先普通依赖，再 devDependencies，
    // 贴近模板和 hooks 最常见的引导流程。
    if ((options?.dependencies?.length ?? 0) > 0) {
      commands.push(
        await installCommand(options?.dependencies ?? [], {
          manager: options?.manager,
          cwd: options?.cwd,
        }),
      );
    }
    if ((options?.devDependencies?.length ?? 0) > 0) {
      commands.push(
        await installCommand(options?.devDependencies ?? [], {
          manager: options?.manager,
          cwd: options?.cwd,
          dev: true,
        }),
      );
    }
    return commands;
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
    detect: async (cwd) => {
      await policy.assertPmAllowed('detect');
      const result = await host.call<{ manager: string }>('host.pm.detect', {
        cwd: cwd ?? base.cwd,
      });
      return result.manager;
    },
    supportedManagers: async () => {
      await policy.assertPmAllowed('supportedManagers');
      const result = await host.call<{ managers: string[] }>('host.pm.supportedManagers');
      return Array.isArray(result.managers) ? result.managers : [];
    },
    spec: async (managerOrCwd) => {
      await policy.assertPmAllowed('spec');
      // `spec()` 故意支持两种调用方式：
      // - 传显式 manager：本地直接返回，不走 host round-trip
      // - 传 cwd/undefined：先让 host 探测当前项目使用的 manager
      if (managerOrCwd && isSupportedPm(managerOrCwd)) {
        return buildSpec(managerOrCwd);
      }
      const result = await host.call<Record<string, unknown>>('host.pm.spec', {
        cwd: managerOrCwd ?? base.cwd,
      });
      return asRecord(result);
    },
    binary,
    lockfile,
    lockfileStrategy: (manager) => `${binary(manager)} uses ${lockfile(manager)}`,
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
      return runExecCommand(await initCommand(options?.manager, options?.cwd), {
        cwd: options?.cwd,
      });
    },
    installAll: async (options) => {
      await policy.assertPmAllowed('installAll');
      return runExecCommand(await installAllCommand(options?.manager, options?.cwd), {
        cwd: options?.cwd,
      });
    },
    install: async (packages, options) => {
      await policy.assertPmAllowed('install');
      return runExecCommand(await installCommand(packages, options), { cwd: options?.cwd });
    },
    remove: async (packages, options) => {
      await policy.assertPmAllowed('remove');
      return runExecCommand(await removeCommand(packages, options), { cwd: options?.cwd });
    },
    update: async (packages, options) => {
      await policy.assertPmAllowed('update');
      return runExecCommand(await updateCommand(packages, options), { cwd: options?.cwd });
    },
    run: async (script, extraArgs, options) => {
      await policy.assertPmAllowed('run');
      if (options?.checked) {
        await tools.requireScript(script, options?.cwd);
      }
      return runExecCommand(
        await runScriptCommand(script, {
          manager: options?.manager,
          cwd: options?.cwd,
          args: extraArgs ?? [],
        }),
        { cwd: options?.cwd, checked: options?.checked },
      );
    },
    publish: async (options) => {
      await policy.assertPmAllowed('publish');
      return runExecCommand(await publishCommand(options), { cwd: options?.cwd });
    },
    command: {
      init: async (manager) => {
        await policy.assertPmAllowed('command.init');
        return initCommand(manager, base.cwd);
      },
      installAll: async (manager) => {
        await policy.assertPmAllowed('command.installAll');
        return installAllCommand(manager, base.cwd);
      },
      install: async (packages, options) => {
        await policy.assertPmAllowed('command.install');
        // 未显式固定 manager 时，优先使用 host 侧 planner，
        // 这样探测结果会与 Rust 侧对该 cwd 的项目解析保持一致。
        if (!options?.manager) {
          const exchange = await hostCall<{ program: string; args: string[] }>(
            'host.pm.command.install',
            {
              cwd: options?.cwd ?? base.cwd,
              packages,
              dev: options?.dev ?? false,
            },
          );
          return exchange.result;
        }
        return installCommand(packages, options);
      },
      remove: async (packages, options) => {
        await policy.assertPmAllowed('command.remove');
        return removeCommand(packages, options);
      },
      update: async (packages, options) => {
        await policy.assertPmAllowed('command.update');
        return updateCommand(packages, options);
      },
      runScript: async (script, options) => {
        await policy.assertPmAllowed('command.runScript');
        if (options?.checked) {
          await tools.requireScript(script, options?.cwd);
        }
        // 与 `command.install` 一样：
        // 未显式指定 manager 时交给 host 探测；显式指定时直接走本地 planner。
        if (!options?.manager) {
          const exchange = await hostCall<{ program: string; args: string[] }>(
            'host.pm.command.runScript',
            {
              cwd: options?.cwd ?? base.cwd,
              script,
              args: options?.args ?? [],
              checked: options?.checked ?? false,
            },
          );
          return exchange.result;
        }
        return runScriptCommand(script, options);
      },
      runScriptChecked: async (script, options) => {
        await policy.assertPmAllowed('command.runScriptChecked');
        await tools.requireScript(script, options?.cwd);
        return runScriptCommand(script, options);
      },
      publish: async (options) => {
        await policy.assertPmAllowed('command.publish');
        return publishCommand(options);
      },
      addDependencyCommands: async (options) => {
        await policy.assertPmAllowed('command.addDependencyCommands');
        return addDependencyCommands(options);
      },
    },
  };

  return tools;
}
