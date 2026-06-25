/**
 * 编译插件共享的运行时探测、事件构造和结果兜底逻辑。
 *
 * 主要导出：createCompilerResult、buildResult、fallbackDevResult、
 * `resolveCompilerRuntime`、`resolveRuntimeWarning`、`detectBuildTool`。
 *
 * 这个模块并不真正执行 Vite/Webpack/Rollup 编译，而是负责：
 * - 判断当前项目有没有可用的编译 runtime
 * - 在 runtime 不完整时生成可解释的 warning 和 fallback 结果
 * - 把 dev/build 生命周期编码成 Rust host 能稳定消费的 bridge events
 *
 * 关键点：
 * - 运行时探测优先贴近用户项目依赖树，而不是 bridge 自身依赖
 * - 事件字段需要同时兼容编译器专属消费方和 bridge 通用事件流
 */
import fs from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';

import type { BridgeEvent } from '../protocol/events.js';
import { asRecord, resolveModuleFromCwd } from '../core/runtime.js';

export type BuildTool = 'vite' | 'webpack' | 'rollup';
export type CompilerAction = 'dev' | 'build';
export type WorkerMode = 'inline_bridge' | 'isolated_worker';

export type ActiveCompiler = {
  tool: BuildTool;
  action: CompilerAction;
  stop: () => Promise<BridgeEvent[] | void>;
};

export interface CompilerAdapterParams {
  cwd: string;
  host: string;
  port: number;
  hmr: boolean | null;
  open: boolean;
  watch: boolean;
  mode: string | null;
  outputDir: string | null;
}

export interface CompilerAdapterContext {
  tool: BuildTool;
  lanConfig: Record<string, unknown>;
  runtimeWarning: string | null;
}

export interface CompilerExecution {
  result: Record<string, unknown>;
  events: BridgeEvent[];
  activeCompiler?: ActiveCompiler;
}

const LEGACY_DEFAULT_DEV_PORT = 8089;
const LEGACY_FALLBACK_PORT_RANGE_START = 18089;
const LEGACY_FALLBACK_PORT_RANGE_END = 18999;

export interface CompilerAdapter {
  tool: BuildTool;
  handleDev(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
  ): Promise<CompilerExecution | null>;
  handleBuild(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
  ): Promise<CompilerExecution | null>;
}

export function createCompilerResult(
  tool: BuildTool,
  action: CompilerAction,
  implementation: 'runtime' | 'fallback',
  extras: Record<string, unknown> = {},
): Record<string, unknown> {
  // 统一生成一个“bridge 能识别、上层 UI 也能直接展示”的结果骨架。
  return {
    accepted: true,
    tool,
    action,
    implementation,
    workerMode: 'inline_bridge' satisfies WorkerMode,
    isolated: false,
    eventSchema: 'lania.compiler.events.v1',
    ...extras,
  };
}

export function buildResult(
  tool: BuildTool,
  watch: boolean,
  mode: string | null,
  outputDir: string | null,
  implementation: 'runtime' | 'fallback',
  warning?: string,
): CompilerExecution {
  // build 结果的价值不只在最终 `result`，还在这串事件：
  // Rust 侧会根据这些事件更新进度、资产列表和日志视图。
  const resolvedOutputDir = outputDir ?? 'dist';
  // build 结果会同时产出“编译器专属事件”和“bridge 通用事件”，
  // 这样 Rust 端既能消费细粒度状态，也能复用通用的资产/进度展示逻辑。
  const events: BridgeEvent[] = [
    compilerStartEvent(tool, 'build', implementation, watch, mode, {
      outputDir: resolvedOutputDir,
    }),
    compilerStatusEvent(tool, 'build', 'resolving', `Resolving ${tool} build graph`),
    {
      method: 'event.progress',
      params: {
        current: 1,
        total: 2,
        message: `Resolving ${tool} build graph`,
      },
    },
    compilerStatusEvent(tool, 'build', watch ? 'watching' : 'building', `Running ${tool} build`),
    compilerAssetEvent(tool, `${resolvedOutputDir}/index.js`, implementation === 'runtime' ? 4096 : 2048, {
      outputDir: resolvedOutputDir,
    }),
    {
      method: 'event.build_asset',
      params: {
        file: `${resolvedOutputDir}/index.js`,
        bytes: implementation === 'runtime' ? 4096 : 2048,
      },
    },
  ];
  if (watch) {
    events.push(
      compilerWatchChangeEvent(tool, 'src/main.ts', 'updated'),
    );
    events.push({
      method: 'event.watch_change',
      params: {
        path: 'src/main.ts',
      },
    });
  }
  if (warning) {
    events.push(compilerIssueEvent(tool, 'warning', warning));
    events.push(logEvent(`[${tool}] ${warning}`, 'warn'));
  }
  events.push(
    compilerDoneEvent(tool, 'build', true, implementation, {
      watch,
      longRunning: watch,
      outputDir: resolvedOutputDir,
    }),
  );

  return {
    result: createCompilerResult(tool, 'build', implementation, {
      watch,
      mode,
      outputDir,
      longRunning: watch,
    }),
    events,
  };
}

export function fallbackDevResult(
  tool: BuildTool,
  host: string,
  port: number,
  warning?: string,
): CompilerExecution {
  // runtime 不可用时，dev 模式仍然会返回一个“可继续工作的最小结果”，
  // 这样调用方至少能拿到 URL、warning 和长运行状态。
  const events: BridgeEvent[] = [
    compilerStartEvent(tool, 'dev', 'fallback', true, 'development', {
      host,
      port,
    }),
    compilerStatusEvent(tool, 'dev', 'starting', `Starting ${tool} dev server`),
    logEvent(`Starting ${tool} dev server`, warning ? 'warn' : 'info'),
    compilerServerReadyEvent(tool, `http://${host}:${port}`, host, port),
    {
      method: 'event.dev_url',
      params: {
        url: `http://${host}:${port}`,
      },
    },
  ];
  if (warning) {
    events.push(compilerIssueEvent(tool, 'warning', warning));
    events.push(logEvent(`[${tool}] ${warning}`, 'warn'));
  }
  events.push(
    compilerDoneEvent(tool, 'dev', true, 'fallback', {
      longRunning: true,
      host,
      port,
    }),
  );
  return {
    result: createCompilerResult(tool, 'dev', 'fallback', {
      mode: 'development',
      host,
      port,
      longRunning: true,
    }),
    events,
  };
}

export async function resolveCompilerRuntime(
  cwd: string,
  tool: BuildTool,
  lanConfig: Record<string, unknown>,
): Promise<any> {
  // runtime 来源有两层：
  // 1. lan.config.buildAdaptors 显式注入的适配器
  // 2. 从用户项目依赖树里解析标准运行时包
  //
  // 这样既允许项目做定制注入，也兼容“直接依赖官方 runtime 包”的默认路径。
  const adaptors = asRecord(lanConfig.buildAdaptors);
  const adaptor = asRecord(adaptors[tool]);

  switch (tool) {
    case 'vite':
      return adaptor.vite ? adaptor.vite : await resolveModuleFromCwd<any>(cwd, 'vite');
    case 'webpack': {
      const webpackRuntime = adaptor.webpack
        ? {
            ...adaptor,
            webpack: unwrapModuleDefault(adaptor.webpack),
            webpackDevServer: unwrapModuleDefault(adaptor.webpackDevServer),
          }
        : await resolveModuleFromCwd<any>(cwd, 'webpack').then((runtime) => {
            const resolvedWebpack = unwrapModuleDefault(runtime);
            return resolvedWebpack
              ? { webpack: resolvedWebpack, webpackDevServer: null }
              : null;
          });
      if (!webpackRuntime?.webpack) {
        return null;
      }
      if (!webpackRuntime.webpackDevServer) {
        webpackRuntime.webpackDevServer = unwrapModuleDefault(
          await resolveModuleFromCwd<any>(cwd, 'webpack-dev-server'),
        );
      }
      return webpackRuntime;
    }
    case 'rollup':
      return adaptor.rollup
        ? adaptor.rollup
        : await resolveModuleFromCwd<any>(cwd, 'rollup');
    default:
      return null;
  }
}

export async function resolveRuntimeWarning(
  cwd: string,
  tool: BuildTool,
  action: CompilerAction = 'build',
  lanConfig: Record<string, unknown> = {},
): Promise<string | null> {
  // warning 只用于提示“明显不对”的状态，而不是做完整依赖诊断。
  // 这里刻意保持启发式，避免把复杂 semver/peer dependency 逻辑塞进 bridge。
  try {
    const manifestPath = path.join(cwd, 'package.json');
    const manifest = JSON.parse(await fs.readFile(manifestPath, 'utf8')) as Record<string, any>;
    const dependencies = {
      ...(asRecord(manifest.dependencies) ?? {}),
      ...(asRecord(manifest.devDependencies) ?? {}),
    };
    const declared = typeof dependencies[tool] === 'string' ? dependencies[tool] : null;
    if (!declared) {
      return `declared dependency \`${tool}\` is missing from package.json`;
    }
    const runtime = await resolveModuleFromCwd<any>(cwd, tool);
    // 这里只做“明显不兼容”的大版本检查，避免把复杂 semver 解析逻辑塞进 bridge。
    const runtimeVersion = typeof runtime?.version === 'string' ? runtime.version : null;
    if (runtimeVersion && declared.startsWith('^')) {
      const expectedMajor = declared.slice(1).split('.')[0];
      const actualMajor = runtimeVersion.split('.')[0];
      if (expectedMajor && actualMajor && expectedMajor !== actualMajor) {
        return `runtime version mismatch: package.json expects ${declared} but loaded ${runtimeVersion}`;
      }
    }
    if (tool === 'webpack' && action === 'dev') {
      const adaptors = asRecord(lanConfig.buildAdaptors);
      const adaptor = asRecord(adaptors.webpack);
      if (!adaptor.webpackDevServer) {
        const devServerDeclared =
          typeof dependencies['webpack-dev-server'] === 'string';
        if (!devServerDeclared) {
          return 'declared dependency `webpack-dev-server` is missing from package.json';
        }
      }
    }
  } catch {
    return null;
  }
  return null;
}

function unwrapModuleDefault<T>(value: T | { default?: T } | null | undefined): T | null {
  // 兼容 ESM/CJS 导出形态，统一把 default export 抹平。
  if (!value) {
    return null;
  }
  const record = asRecord(value as unknown);
  return (record.default ?? value) as T;
}

export function detectBuildTool(config: Record<string, unknown>): BuildTool {
  // 当前只允许显式切到 webpack/rollup；其它情况一律视为 vite。
  const buildTool = config.buildTool;
  if (buildTool === 'webpack' || buildTool === 'rollup') {
    return buildTool;
  }
  return 'vite';
}

export async function findAvailablePort(port: number, host: string): Promise<number> {
  // 端口选择顺序：
  // 1. 优先尝试请求端口
  // 2. 8089 冲突时走历史 fallback 区间，兼容旧项目默认行为
  // 3. 仍然失败再申请临时端口
  if (await canListenOnPort(port, host)) {
    return port;
  }

  // 兼容历史默认端口：8089 被占用时优先尝试旧约定的 fallback 区间，
  // 这样老项目迁移到新 bridge 时端口行为更可预测。
  const fallbackCandidates =
    port === LEGACY_DEFAULT_DEV_PORT
      ? range(
          LEGACY_FALLBACK_PORT_RANGE_START,
          LEGACY_FALLBACK_PORT_RANGE_END,
        )
      : range(Math.max(port + 1, 1024), Math.max(port + 20, 1024));

  for (const candidate of fallbackCandidates) {
    if (candidate === port) {
      continue;
    }
    if (await canListenOnPort(candidate, host)) {
      return candidate;
    }
  }

  return await allocateEphemeralPort(host);
}

async function canListenOnPort(port: number, host: string): Promise<boolean> {
  return await new Promise<boolean>((resolve) => {
    const server = net.createServer();
    server.once('error', () => resolve(false));
    server.listen(port, host, () => {
      server.close(() => resolve(true));
    });
  });
}

async function allocateEphemeralPort(host: string): Promise<number> {
  return await new Promise<number>((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, host, () => {
      const address = server.address();
      const resolvedPort =
        address && typeof address === 'object' ? address.port : 0;
      server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(resolvedPort);
      });
    });
  });
}

function range(start: number, end: number): number[] {
  const values: number[] = [];
  for (let candidate = start; candidate <= end; candidate += 1) {
    values.push(candidate);
  }
  return values;
}

export function logEvent(
  message: string,
  level: 'info' | 'warn' = 'info',
): BridgeEvent {
  // 通用日志事件，供 fallback/runtime 两条路径复用。
  return {
    method: 'event.log',
    params: {
      level,
      message,
    },
  };
}

export function compilerStartEvent(
  tool: BuildTool,
  action: CompilerAction,
  implementation: 'runtime' | 'fallback',
  watch: boolean,
  mode: string | null,
  extra: Record<string, unknown> = {},
): BridgeEvent {
  // 明确标记一次编译生命周期开始，以及是 runtime 还是 fallback 实现。
  return {
    method: 'event.compiler_start',
    params: {
      tool,
      action,
      implementation,
      workerMode: 'inline_bridge',
      isolated: false,
      watch,
      mode,
      ...extra,
    },
  };
}

export function compilerStatusEvent(
  tool: BuildTool,
  action: CompilerAction,
  stage: 'resolving' | 'starting' | 'building' | 'watching' | 'stopping',
  message: string,
): BridgeEvent {
  // 阶段状态事件用于驱动 UI 上的“当前进度/当前阶段”展示。
  return {
    method: 'event.compiler_status',
    params: {
      tool,
      action,
      stage,
      message,
    },
  };
}

export function compilerServerReadyEvent(
  tool: BuildTool,
  url: string,
  host: string,
  port: number,
): BridgeEvent {
  // dev server ready 事件是上层拿访问地址的主要依据。
  return {
    method: 'event.compiler_server_ready',
    params: {
      tool,
      url,
      host,
      port,
    },
  };
}

export function compilerAssetEvent(
  tool: BuildTool,
  file: string,
  bytes: number,
  extra: Record<string, unknown> = {},
): BridgeEvent {
  // build 产物事件既可用于列表展示，也可用于后续汇总产物大小。
  return {
    method: 'event.compiler_asset',
    params: {
      tool,
      file,
      bytes,
      ...extra,
    },
  };
}

export function compilerIssueEvent(
  tool: BuildTool,
  severity: 'warning' | 'error',
  message: string,
  extra: Record<string, unknown> = {},
): BridgeEvent {
  // warning/error 统一编码成 issue 事件，避免调用方自己从日志文本里反推严重度。
  return {
    method: 'event.compiler_issue',
    params: {
      tool,
      severity,
      message,
      ...extra,
    },
  };
}

export function compilerWatchChangeEvent(
  tool: BuildTool,
  path: string,
  change: 'created' | 'updated' | 'deleted',
): BridgeEvent {
  // watch 模式下文件变化事件，方便 host 侧增量更新“最近变更”提示。
  return {
    method: 'event.compiler_watch_change',
    params: {
      tool,
      path,
      change,
    },
  };
}

export function compilerDoneEvent(
  tool: BuildTool,
  action: CompilerAction,
  success: boolean,
  implementation: 'runtime' | 'fallback',
  extra: Record<string, unknown> = {},
): BridgeEvent {
  // 生命周期结束事件；调用方通常以它为一次 dev/build 流程的收尾标记。
  return {
    method: 'event.compiler_done',
    params: {
      tool,
      action,
      success,
      implementation,
      workerMode: 'inline_bridge',
      isolated: false,
      ...extra,
    },
  };
}
