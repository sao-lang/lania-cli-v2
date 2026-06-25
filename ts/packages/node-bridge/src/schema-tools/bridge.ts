/**
 * `tools.bridge`：Node 侧本地 bridge 能力的请求构造与调用封装。
 *
 * 这一层有两个用途：
 * - 给 schema 暴露一组“显式 request builder + call”能力，方便手动构造协议请求
 * - 把 config/compiler/lint/system/commit 这些常见 bridge 方法整理成语义化入口
 *
 * 对调用方来说，这里更像“本地协议客户端”，而不是业务插件实现本身。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type { BridgeRequest } from '../protocol/request.js';
import type { BridgeExchange, BridgeResponse } from '../protocol/response.js';
import { commitizenPlugin } from '../plugins/commitizen.js';
import { commitlintPlugin } from '../plugins/commitlint.js';
import { compilerPlugin } from '../plugins/compiler.js';
import { configPlugin } from '../plugins/config.js';
import { lintPlugin } from '../plugins/lint.js';
import { systemPlugin } from '../plugins/system.js';

export interface BridgeTools {
  request: (method: string, params?: Record<string, unknown>) => BridgeRequest<Record<string, unknown>>;
  handshakeRequest: () => BridgeRequest<Record<string, unknown>>;
  pingRequest: () => BridgeRequest<Record<string, unknown>>;
  shutdownRequest: () => BridgeRequest<Record<string, unknown>>;
  metricsRequest: (cwd?: string) => BridgeRequest<Record<string, unknown>>;
  subscribeRequest: (cwd?: string) => BridgeRequest<Record<string, unknown>>;
  loadLanConfigRequest: (cwd?: string) => BridgeRequest<Record<string, unknown>>;
  loadToolConfigRequest: (cwd: string | undefined, tool: string) => BridgeRequest<Record<string, unknown>>;
  compilerDevRequest: (cwd?: string, port?: number) => BridgeRequest<Record<string, unknown>>;
  compilerBuildRequest: (cwd?: string, watch?: boolean) => BridgeRequest<Record<string, unknown>>;
  compilerBuildWithOptionsRequest: (
    cwd?: string,
    watch?: boolean,
    mode?: string,
    outputDir?: string,
  ) => BridgeRequest<Record<string, unknown>>;
  compilerStopRequest: () => BridgeRequest<Record<string, unknown>>;
  lintRunRequest: (
    cwd?: string,
    fix?: boolean,
    concurrency?: number,
  ) => BridgeRequest<Record<string, unknown>>;
  systemListCommandsRequest: (
    cwd?: string,
    options?: Record<string, unknown>,
  ) => BridgeRequest<Record<string, unknown>>;
  commitizenRunRequest: (
    cwd: string | undefined,
    kind: string,
    scope?: string,
    subject?: string,
  ) => BridgeRequest<Record<string, unknown>>;
  commitlintRunRequest: (
    cwd: string | undefined,
    message: string,
  ) => BridgeRequest<Record<string, unknown>>;
  call: (
    requestOrMethod: BridgeRequest<Record<string, unknown>> | string,
    params?: Record<string, unknown>,
  ) => Promise<BridgeExchange>;
  callAsync: (
    requestOrMethod: BridgeRequest<Record<string, unknown>> | string,
    params?: Record<string, unknown>,
  ) => Promise<BridgeExchange>;
  openCall: (
    requestOrMethod: BridgeRequest<Record<string, unknown>> | string,
    params?: Record<string, unknown>,
  ) => Promise<BridgeExchange>;
  subscribeEvents: (callback: (event: BridgeEvent) => void | Promise<void>) => () => void;
  metricsSnapshot: () => Record<string, unknown>;
  shutdown: () => Promise<BridgeExchange>;
  shutdownAsync: () => Promise<BridgeExchange>;
  usingProcessTransport: () => boolean;
  supportedEvents: () => string[];
  timeout: () => number;
  raw: {
    request: (
      method: string,
      params?: Record<string, unknown>,
    ) => BridgeRequest<Record<string, unknown>>;
    call: (method: string, params?: Record<string, unknown>) => Promise<BridgeExchange>;
  };
  config: {
    loadLan: (cwd?: string) => Promise<BridgeExchange>;
    loadTool: (tool: string, cwd?: string) => Promise<BridgeExchange>;
  };
  compiler: {
    dev: (options?: Record<string, unknown>) => Promise<BridgeExchange>;
    build: (options?: Record<string, unknown>) => Promise<BridgeExchange>;
    stop: () => Promise<BridgeExchange>;
  };
  lint: {
    run: (options?: Record<string, unknown>) => Promise<BridgeExchange>;
  };
  system: {
    listCommands: (options?: Record<string, unknown>) => Promise<BridgeExchange>;
  };
  commit: {
    commitizen: (options: Record<string, unknown>) => Promise<BridgeExchange>;
    commitlint: (message: string, options?: Record<string, unknown>) => Promise<BridgeExchange>;
  };
}

export const LOCAL_BRIDGE_METHODS = [
  'bridge.ping',
  'bridge.shutdown',
  'bridge.metrics',
  'bridge.subscribe',
  'bridge.heartbeat',
  'plugins.resolve',
  'system.listCommands',
] as const;

// 这些是本地 bridge 在当前阶段明确支持推送给调用方的事件集合。
export const LOCAL_BRIDGE_EVENTS: BridgeEvent['method'][] = [
  'event.ready',
  'event.log',
  'event.progress',
  'event.dev_url',
  'event.build_asset',
  'event.compiler_start',
  'event.compiler_status',
  'event.compiler_server_ready',
  'event.compiler_asset',
  'event.compiler_issue',
  'event.compiler_watch_change',
  'event.compiler_done',
  'event.lint_start',
  'event.lint_file',
  'event.lint_result',
  'event.lint_summary',
  'event.watch_change',
  'event.shutdown',
  'event.heartbeat',
] as const;

export const LOCAL_BRIDGE_TIMEOUT_MS = 30_000;

export function createBridgeTools(options: {
  cwd: string;
  buildBridgeRequest: (
    method: string,
    params?: Record<string, unknown>,
  ) => BridgeRequest<Record<string, unknown>>;
  invokeBridge: (
    requestOrMethod: BridgeRequest<Record<string, unknown>> | string,
    params?: Record<string, unknown>,
  ) => Promise<BridgeExchange>;
  subscribeEvents: (callback: (event: BridgeEvent) => void | Promise<void>) => () => void;
  metricsSnapshot: () => Record<string, unknown>;
}): BridgeTools {
  // `raw` 保留最接近底层协议的 request/call 入口，适合需要手动拼 method 的调用方。
  const bridgeRaw: BridgeTools['raw'] = {
    request: options.buildBridgeRequest,
    call: (method, params) => options.invokeBridge(method, params),
  };

  return {
    request: options.buildBridgeRequest,
    handshakeRequest: () =>
      options.buildBridgeRequest('bridge.handshake', {
        protocolVersion: '0.1.0',
        transport: 'stdio',
        encoding: 'json',
        hostName: '@lania-cli/schema-tools',
      }),
    pingRequest: () => options.buildBridgeRequest('bridge.ping', {}),
    shutdownRequest: () => options.buildBridgeRequest('bridge.shutdown', {}),
    metricsRequest: (cwd) => options.buildBridgeRequest('bridge.metrics', { cwd: cwd ?? options.cwd }),
    subscribeRequest: (cwd) => options.buildBridgeRequest('bridge.subscribe', { cwd: cwd ?? options.cwd }),
    loadLanConfigRequest: (cwd) =>
      options.buildBridgeRequest('config.loadLan', { cwd: cwd ?? options.cwd }),
    loadToolConfigRequest: (cwd, tool) =>
      options.buildBridgeRequest('config.loadTool', { cwd: cwd ?? options.cwd, tool }),
    compilerDevRequest: (cwd, port) =>
      options.buildBridgeRequest('compiler.dev', {
        cwd: cwd ?? options.cwd,
        ...(typeof port === 'number' ? { port } : {}),
      }),
    compilerBuildRequest: (cwd, watch) =>
      options.buildBridgeRequest('compiler.build', { cwd: cwd ?? options.cwd, watch: watch ?? false }),
    compilerBuildWithOptionsRequest: (cwd, watch, mode, outputDir) =>
      options.buildBridgeRequest('compiler.build', {
        cwd: cwd ?? options.cwd,
        watch: watch ?? false,
        ...(typeof mode === 'string' ? { mode } : {}),
        ...(typeof outputDir === 'string' ? { outputDir } : {}),
      }),
    compilerStopRequest: () => options.buildBridgeRequest('compiler.stop', {}),
    lintRunRequest: (cwd, fix, concurrency) =>
      options.buildBridgeRequest('lint.run', {
        cwd: cwd ?? options.cwd,
        fix: fix ?? false,
        ...(typeof concurrency === 'number' ? { concurrency } : {}),
      }),
    systemListCommandsRequest: (cwd, commandOptions) =>
      options.buildBridgeRequest('system.listCommands', {
        cwd: cwd ?? options.cwd,
        ...(commandOptions ?? {}),
      }),
    commitizenRunRequest: (cwd, kind, scope, subject) =>
      options.buildBridgeRequest('commitizen.run', {
        cwd: cwd ?? options.cwd,
        kind,
        ...(typeof scope === 'string' ? { scope } : {}),
        ...(typeof subject === 'string' ? { subject } : {}),
      }),
    commitlintRunRequest: (cwd, message) =>
      options.buildBridgeRequest('commitlint.run', { cwd: cwd ?? options.cwd, message }),
    call: (requestOrMethod, params) => options.invokeBridge(requestOrMethod, params),
    callAsync: (requestOrMethod, params) => options.invokeBridge(requestOrMethod, params),
    openCall: (requestOrMethod, params) => options.invokeBridge(requestOrMethod, params),
    subscribeEvents: options.subscribeEvents,
    metricsSnapshot: options.metricsSnapshot,
    shutdown: () => options.invokeBridge('bridge.shutdown', {}),
    shutdownAsync: () => options.invokeBridge('bridge.shutdown', {}),
    usingProcessTransport: () => false,
    supportedEvents: () => [...LOCAL_BRIDGE_EVENTS],
    timeout: () => LOCAL_BRIDGE_TIMEOUT_MS,
    raw: bridgeRaw,
    config: {
      loadLan: (cwd) => bridgeRaw.call('config.loadLan', { cwd: cwd ?? options.cwd }),
      loadTool: (tool, cwd) => bridgeRaw.call('config.loadTool', { cwd: cwd ?? options.cwd, tool }),
    },
    compiler: {
      dev: (opts) => bridgeRaw.call('compiler.dev', { cwd: options.cwd, ...(opts ?? {}) }),
      build: (opts) => bridgeRaw.call('compiler.build', { cwd: options.cwd, ...(opts ?? {}) }),
      stop: () => bridgeRaw.call('compiler.stop', {}),
    },
    lint: {
      run: (opts) => bridgeRaw.call('lint.run', { cwd: options.cwd, ...(opts ?? {}) }),
    },
    system: {
      listCommands: (opts) => bridgeRaw.call('system.listCommands', { cwd: options.cwd, ...(opts ?? {}) }),
    },
    commit: {
      commitizen: (opts) => bridgeRaw.call('commitizen.run', { cwd: options.cwd, ...(opts ?? {}) }),
      commitlint: (message, opts) =>
        bridgeRaw.call('commitlint.run', { cwd: options.cwd, message, ...(opts ?? {}) }),
    },
  };
}

export async function dispatchBuiltinBridge(
  method: string,
  params: Record<string, unknown>,
  cwd: string,
): Promise<BridgeExchange> {
  // 这里实现的是 Node 侧内建 bridge 方法分发。
  // 如果 method 不属于这些本地插件，就说明应由更外层桥接到 host 或直接报 unsupported。
  const id = `local-${Math.random().toString(16).slice(2)}`;
  const context = { cwd };

  const handled = method.startsWith('config.')
    ? await configPlugin.handle(method, params)
    : method.startsWith('compiler.')
      ? await compilerPlugin.handle(method, params)
      : method.startsWith('lint.')
        ? await lintPlugin.handle(method, params)
        : method.startsWith('system.')
          ? await systemPlugin.handle(method, params, context as any)
        : method.startsWith('commitizen.')
          ? await commitizenPlugin.handle(method, params, context as any)
          : method.startsWith('commitlint.')
            ? await commitlintPlugin.handle(method, params, context as any)
            : null;

  if (!handled) {
    throw new Error(`unsupported bridge method in schema tools: ${method}`);
  }

  const response: BridgeResponse = { id, result: handled.result };
  return { response, events: handled.events };
}
