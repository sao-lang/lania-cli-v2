/**
 * Schema Tools（v2.3）
 *
 * 这是 dynamic command handler 和 inline hook 运行时拿到的 `ctx.tools`。
 * 它本质上是一层统一的工具装配入口：
 * - 把本地工具、bridge 工具、host 工具挂到同一个对象上；
 * - 在调用边界统一做策略校验、审计埋点和事件转发；
 * - 尽量让上层调用方不用关心能力最终落在 Node 侧还是 Rust 侧。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type { BridgeRequest } from '../protocol/request.js';
import type { BridgeExchange } from '../protocol/response.js';
import {
  createBridgeTools,
  dispatchBuiltinBridge,
  LOCAL_BRIDGE_EVENTS,
  LOCAL_BRIDGE_METHODS,
  type BridgeTools,
} from '../schema-tools/bridge.js';
import { createConfigTools, type ConfigTools } from '../schema-tools/config.js';
import { createEnvTools, type EnvTools } from '../schema-tools/env.js';
import { createExecTools, type ExecTools } from '../schema-tools/exec.js';
import { createFsTools, type FsTools } from '../schema-tools/fs.js';
import { createGitTools, type GitTools } from '../schema-tools/git.js';
import { createHostTools, type HostTools } from '../schema-tools/host.js';
import { createInteractionTools, type InteractionTools } from '../schema-tools/interaction.js';
import { createJsonTools, type JsonTools } from '../schema-tools/json.js';
import { createLogTools, type LogTools } from '../schema-tools/log.js';
import { createPathTools, type PathTools } from '../schema-tools/path.js';
import { createToolsPolicyManager } from '../schema-tools/policy.js';
import { createPackageManagerTools, type PackageManagerTools } from '../schema-tools/pm.js';
import { createProgressTools, type ProgressTools } from '../schema-tools/progress.js';
import { createResultTools, type ResultTools } from '../schema-tools/result.js';
import { createScaffoldTools, type ScaffoldTools } from '../schema-tools/scaffold.js';
import { createTaskTools, type TaskTools } from '../schema-tools/tasks.js';
import { createTextTools, type TextTools } from '../schema-tools/text.js';
import type { SchemaToolContext } from '../schema-tools/types.js';
import { createWorkspaceTools, type WorkspaceTools } from '../schema-tools/workspace.js';
import type {
  ProductContext as DynamicProductContext,
  RuntimeContext as DynamicRuntimeContext,
  ScaffoldPlan,
} from '../plugins/dynamic-commands/types.js';

export type { BridgeTools } from '../schema-tools/bridge.js';
export type { ConfigTools } from '../schema-tools/config.js';
export type { EnvTools } from '../schema-tools/env.js';
export type {
  ExecCommandBuilder,
  ExecCommandInput,
  ExecRunInvokeOptions,
  ExecRunResult,
  ExecTools,
} from '../schema-tools/exec.js';
export type { FsTools } from '../schema-tools/fs.js';
export type {
  GitCommitLogEntry,
  GitCommitLogOptionsInput,
  GitExecCommand,
  GitRemoteInfo,
  GitStatusResult,
  GitTools,
  GitUpstreamInfo,
  GitUserInfo,
} from '../schema-tools/git.js';
export type { HostExchange, HostTools } from '../schema-tools/host.js';
export type {
  InteractionFlow,
  InteractionPromptOptions,
  InteractionPromptState,
  InteractionQuestion,
  InteractionTools,
} from '../schema-tools/interaction.js';
export type { JsonTools } from '../schema-tools/json.js';
export type { LogTools } from '../schema-tools/log.js';
export type { PathTools } from '../schema-tools/path.js';
export type { PackageManagerTools } from '../schema-tools/pm.js';
export type { ProgressTools } from '../schema-tools/progress.js';
export type { ResultTools } from '../schema-tools/result.js';
export type {
  ScaffoldDependencyPlanResult,
  ScaffoldRenderResult,
  ScaffoldTemplateFile,
  ScaffoldTools,
} from '../schema-tools/scaffold.js';
export type {
  TaskExecutor,
  TaskExecutorOptions,
  TaskExecutorResult,
  TaskExecutorTask,
  TaskTools,
} from '../schema-tools/tasks.js';
export type {
  TextColorValue,
  TextRenderOptions,
  TextStyleHandle,
  TextTools,
} from '../schema-tools/text.js';
export type { HookKind, SchemaToolContext } from '../schema-tools/types.js';
export type { WorkspaceTools } from '../schema-tools/workspace.js';

export interface SchemaTools {
  config: ConfigTools;
  bridge: BridgeTools;
  compiler: BridgeTools['compiler'];
  lint: BridgeTools['lint'];
  path: PathTools;
  workspace: WorkspaceTools;
  env: EnvTools;
  json: JsonTools;
  result: ResultTools;
  text: TextTools;
  host: HostTools;
  exec: ExecTools;
  git: GitTools;
  pm: PackageManagerTools;
  scaffold: ScaffoldTools;
  fs: FsTools;
  log: LogTools;
  tasks: TaskTools;
  progress: ProgressTools;
  interaction: InteractionTools;
}

/**
 * 创建一次命令执行所使用的完整工具集。
 *
 * 这里会同时完成几类工作：
 * - 根据当前 `SchemaToolContext` 构造各个工具实例；
 * - 把 runtime / scaffold / product 等额外上下文注入到需要的工具；
 * - 给工具方法补上统一的审计包装；
 * - 准备 bridge 请求构造、事件订阅和本地内建方法分发逻辑。
 *
 * 返回值可以视为“本次执行的工具快照”，后续 handler / hook 都通过它访问环境能力。
 */
export function createSchemaTools(
  ctx: SchemaToolContext,
  extras?: {
    scaffold?: ScaffoldPlan;
    runtime?: DynamicRuntimeContext;
    product?: DynamicProductContext;
  },
): SchemaTools {
  // 保留一份稳定的上下文副本，避免闭包读到外部后续改写的引用。
  const base = { ...ctx };
  const policy = createToolsPolicyManager(base);
  const requestId = createRequestIdFactory('tools-bridge');
  const toolEvents = base.events ?? [];
  const TOOL_AUDIT_WRAPPED = Symbol('toolAuditWrapped');

  const emitToolAuditEvent = (
    tool: string,
    method: string,
    startedAt: number,
    ok: boolean,
    detail?: Record<string, unknown>,
  ) => {
    // 工具调用审计统一落到事件流里，便于排查 hook/command 内部到底调用了什么。
    toolEvents.push({
      method: 'event.log',
      params: {
        level: ok ? 'debug' : 'warn',
        target: 'schema.tools',
        message: `${ok ? 'ok' : 'fail'} tools.${tool}.${method}`,
        traceId: base.traceId,
        phase: 'tool_call',
        operation: `tools.${tool}.${method}`,
        tool,
        methodName: method,
        cwd: base.cwd,
        durationMs: Date.now() - startedAt,
        ok,
        mount: base.mount,
        path: base.path,
        commandHandlerId: base.commandHandlerId,
        hook: base.hook,
        hookKind: base.hookKind,
        hookSource: base.hookSource,
        ...detail,
      },
    });
  };

  const wrapToolCall = <TArgs extends unknown[], TResult>(
    tool: string,
    method: string,
    fn: (...args: TArgs) => Promise<TResult> | TResult,
    detailFactory?: (...args: TArgs) => Record<string, unknown> | undefined,
  ) => {
    // 把“计时 + 成功失败记录 + 审计事件”这种横切逻辑收口到一个包装器里，
    // 避免每个工具实现自己重复埋点。
    const wrapped = async (...args: TArgs): Promise<TResult> => {
      const startedAt = Date.now();
      try {
        const result = await fn(...args);
        emitToolAuditEvent(tool, method, startedAt, true, detailFactory?.(...args));
        return result;
      } catch (error) {
        emitToolAuditEvent(tool, method, startedAt, false, {
          ...detailFactory?.(...args),
          error: error instanceof Error ? { message: error.message } : { message: String(error) },
        });
        throw error;
      }
    };
    Object.defineProperty(wrapped, TOOL_AUDIT_WRAPPED, { value: true });
    return wrapped;
  };

  const wrapAsyncMethod = <
    TObject extends Record<string, any>,
    TKey extends keyof TObject & string,
    TArgs extends unknown[],
    TResult,
  >(
    target: TObject,
    key: TKey,
    tool: string,
    method: string,
    detailFactory?: (...args: TArgs) => Record<string, unknown> | undefined,
  ) => {
    // 对已知的异步方法做定向包装，方便附带额外的 detail 信息。
    const original = target[key] as (...args: TArgs) => Promise<TResult>;
    target[key] = wrapToolCall(tool, method, original.bind(target), detailFactory) as TObject[TKey];
  };

  const wrapSyncMethod = <
    TObject extends Record<string, any>,
    TKey extends keyof TObject & string,
    TArgs extends unknown[],
    TResult,
  >(
    target: TObject,
    key: TKey,
    tool: string,
    method: string,
    detailFactory?: (...args: TArgs) => Record<string, unknown> | undefined,
  ) => {
    // 同步方法使用与异步方法一致的审计模型，保证日志口径统一。
    const original = target[key] as (...args: TArgs) => TResult;
    target[key] = ((...args: TArgs) => {
      const startedAt = Date.now();
      try {
        const result = original.apply(target, args);
        emitToolAuditEvent(tool, method, startedAt, true, detailFactory?.(...args));
        return result;
      } catch (error) {
        emitToolAuditEvent(tool, method, startedAt, false, {
          ...detailFactory?.(...args),
          error: error instanceof Error ? { message: error.message } : { message: String(error) },
        });
        throw error;
      }
    }) as TObject[TKey];
    Object.defineProperty(target[key] as object, TOOL_AUDIT_WRAPPED, { value: true });
  };

  const wrapNamespaceMethods = (
    value: unknown,
    tool: string,
    prefix = '',
    seen = new WeakSet<object>(),
  ) => {
    // 递归包装命名空间下的可调用方法，避免像 `tools.bridge.raw.call`
    // 这类多层嵌套入口漏掉审计。
    if (!value || typeof value !== 'object') {
      return;
    }
    if (seen.has(value)) {
      return;
    }
    seen.add(value);
    for (const [key, entry] of Object.entries(value)) {
      const methodName = prefix ? `${prefix}.${key}` : key;
      if (typeof entry === 'function') {
        if ((entry as Record<PropertyKey, unknown>)[TOOL_AUDIT_WRAPPED]) {
          continue;
        }
        const original = entry;
        const wrapped = function (this: unknown, ...args: unknown[]) {
          const startedAt = Date.now();
          try {
            const result = original.apply(this, args);
            if (result && typeof (result as Promise<unknown>).then === 'function') {
              return Promise.resolve(result)
                .then((resolved) => {
                  wrapNamespaceMethods(resolved, tool, methodName, seen);
                  emitToolAuditEvent(tool, methodName, startedAt, true);
                  return resolved;
                })
                .catch((error) => {
                  emitToolAuditEvent(tool, methodName, startedAt, false, {
                    error:
                      error instanceof Error
                        ? { message: error.message }
                        : { message: String(error) },
                  });
                  throw error;
                });
            }
            wrapNamespaceMethods(result, tool, methodName, seen);
            emitToolAuditEvent(tool, methodName, startedAt, true);
            return result;
          } catch (error) {
            emitToolAuditEvent(tool, methodName, startedAt, false, {
              error:
                error instanceof Error ? { message: error.message } : { message: String(error) },
            });
            throw error;
          }
        };
        Object.defineProperty(wrapped, TOOL_AUDIT_WRAPPED, { value: true });
        (value as Record<string, unknown>)[key] = wrapped;
      } else if (entry && typeof entry === 'object') {
        wrapNamespaceMethods(entry, tool, methodName, seen);
      }
    }
  };

  const bridgeMetrics = {
    requests: 0,
    events: 0,
  };
  const bridgeSubscribers = new Set<(event: BridgeEvent) => void | Promise<void>>();

  const emitBridgeEvents = async (events: BridgeEvent[]) => {
    bridgeMetrics.events += events.length;
    for (const event of events) {
      for (const callback of bridgeSubscribers) {
        await callback(event);
      }
    }
  };

  const buildBridgeRequest = (
    method: string,
    params?: Record<string, unknown>,
  ): BridgeRequest<Record<string, unknown>> => {
    // 本地 bridge 调用也先归一成标准 request，保持与 host 侧协议一致。
    const request = { id: requestId(), method, params: params ?? {} };
    return request;
  };

  const dispatchLocalBridgeRequest = async (
    request: BridgeRequest<Record<string, unknown>>,
  ): Promise<BridgeExchange> => {
    // 这里处理 Node 侧内建 bridge 方法；如果不是内建方法，再交给 builtin bridge 分发。
    bridgeMetrics.requests += 1;
    if (request.method === 'bridge.ping') {
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            bridgeName: '@lania-cli/node-bridge',
          },
        },
        events: [],
      };
    }
    if (request.method === 'bridge.metrics') {
      return {
        response: {
          id: request.id,
          result: {
            ...bridgeMetrics,
            plugins: ['config', 'compiler', 'lint', 'commitizen', 'commitlint'],
            rejectedPlugins: [],
          },
        },
        events: [],
      };
    }
    if (request.method === 'bridge.subscribe') {
      return {
        response: {
          id: request.id,
          result: {
            accepted: true,
            events: [...LOCAL_BRIDGE_EVENTS],
            mode: 'request_response_stream',
          },
        },
        events: [],
      };
    }
    if (request.method === 'bridge.shutdown') {
      return {
        response: {
          id: request.id,
          result: {
            accepted: true,
            stopped: true,
          },
        },
        events: [
          {
            method: 'event.shutdown',
            params: { reason: 'requested' },
          },
        ],
      };
    }
    if (request.method === 'bridge.heartbeat') {
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            ts: Date.now(),
          },
        },
        events: [
          {
            method: 'event.heartbeat',
            params: { ts: Date.now() },
          },
        ],
      };
    }
    if (request.method === 'plugins.resolve') {
      return {
        response: {
          id: request.id,
          result: {
            ok: true,
            cwd: String(request.params.cwd ?? base.cwd),
            plugins: ['config', 'compiler', 'lint', 'commitizen', 'commitlint'],
            methods: [...LOCAL_BRIDGE_METHODS],
            rejectedPlugins: [],
          },
        },
        events: [],
      };
    }
    return dispatchBuiltinBridge(
      request.method,
      request.params ?? {},
      String(request.params.cwd ?? base.cwd),
    );
  };

  const invokeBridge = async (
    requestOrMethod: BridgeRequest<Record<string, unknown>> | string,
    params?: Record<string, unknown>,
  ): Promise<BridgeExchange> => {
    // bridge 是少数同时涉及策略校验、请求构造和事件广播的入口，这里统一收口。
    const request =
      typeof requestOrMethod === 'string'
        ? buildBridgeRequest(requestOrMethod, params)
        : requestOrMethod;
    await policy.assertBridgeMethodAllowed(request.method);
    const exchange = await dispatchLocalBridgeRequest(request);
    await emitBridgeEvents(exchange.events);
    return exchange;
  };

  const tools: SchemaTools = {
    config: createConfigTools(base, policy),
    bridge: createBridgeTools({
      cwd: base.cwd,
      buildBridgeRequest,
      invokeBridge,
      subscribeEvents: (callback) => {
        bridgeSubscribers.add(callback);
        return () => {
          bridgeSubscribers.delete(callback);
        };
      },
      metricsSnapshot: () => ({
        ...bridgeMetrics,
        methods: [...LOCAL_BRIDGE_METHODS],
        events: [...LOCAL_BRIDGE_EVENTS],
      }),
    }),
    compiler: undefined as unknown as BridgeTools['compiler'],
    lint: undefined as unknown as BridgeTools['lint'],
    path: createPathTools(),
    workspace: createWorkspaceTools(base, policy),
    env: createEnvTools(base),
    json: createJsonTools(policy),
    result: createResultTools(),
    text: createTextTools(),
    host: createHostTools(base, policy),
    exec: createExecTools(base, policy),
    git: createGitTools(base, policy),
    pm: createPackageManagerTools(base, policy),
    scaffold: undefined as unknown as ScaffoldTools,
    fs: createFsTools(base, policy),
    log: createLogTools(base, policy),
    tasks: createTaskTools(base, policy),
    progress: createProgressTools(base, policy),
    interaction: createInteractionTools(base, policy),
  };

  // compiler / lint 当前只是 bridge 的语义化别名，单独暴露是为了让调用方 API 更直观。
  tools.compiler = tools.bridge.compiler;
  tools.lint = tools.bridge.lint;
  tools.scaffold = createScaffoldTools({
    base,
    pm: tools.pm,
    scaffold: extras?.scaffold,
    runtime: extras?.runtime,
    product: extras?.product,
  });
  tools.bridge.raw.call = wrapToolCall(
    'bridge',
    'raw.call',
    tools.bridge.raw.call.bind(tools.bridge.raw),
    (method) => ({ bridgeMethod: method }),
  );

  wrapAsyncMethod(tools.host, 'call', 'host', 'call', (method) => ({ hostMethod: method }));
  wrapAsyncMethod(tools.pm, 'detect', 'pm', 'detect');
  wrapSyncMethod(tools.text, 'render', 'text', 'render');
  wrapSyncMethod(tools.text, 'style', 'text', 'style');
  wrapSyncMethod(tools.text, 'rgb', 'text', 'rgb');
  wrapSyncMethod(tools.text, 'hsl', 'text', 'hsl');

  for (const [tool, value] of Object.entries(tools)) {
    wrapNamespaceMethods(value, tool);
  }

  return tools;
}

function createRequestIdFactory(prefix: string) {
  // 只要求“单次执行内唯一”，不追求跨进程全局可排序。
  let seq = 0;
  const seed = `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}`;
  return () => `${prefix}-${seed}-${++seq}`;
}
