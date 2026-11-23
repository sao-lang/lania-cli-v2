/**
 * Hub endpoint 模式的 stdio 入口（文档 §5.5 E.1 / 决策 #12）。
 *
 * 与默认的「1:1 直连帧」模式的差别：
 * - 帧层完全复用 `@lania-cli-v2/bridge-protocol`（Bridge Hub 的 BusFrame），
 *   bridge 侧不再自研帧协议（决策 #12 的「协议逻辑不允许写两遍」）；
 * - 端点以 `announce` 帧声明自己提供的能力（切片方法前缀），
 *   请求以 `request` 帧进来，响应以 `response` 帧出去；
 * - 事件以 `publish`/`event` 帧流转。
 *
 * 帧的**写出**与默认 1:1 路径共用 `envelope.ts` 里唯一的 `writeEnvelope`：
 * 生成骨架 `EndpointRuntime` 的 `write` 被统一重定向到这条写链，从而两条路径
 * 不再各写一份 jsonl 序列化逻辑。生成物的 BusFrame 用 `kind` 判别、且无 handshake
 * 帧，与 1:1 握手语义不兼容，因此 1:1 路径仍用 `IncomingEnvelope`，但共享写出实现。
 *
 * 切换方式：`LANIA_BRIDGE_MODE=hub`（默认仍是 1:1 模式，保证零行为变化）。
 */
import {
  EndpointRuntime,
  endpointIdFromEnv,
  type BusFrame,
  type EndpointHooks,
} from '@lania-cli-v2/bridge-protocol';
import type { BridgePluginEvent } from '../core/bridge-plugin.js';
import { BRIDGE_METHODS } from './protocol-catalog.js';

import { handleExchange } from './index.js';
import { loadSlices, resolveSliceSpec } from '../core/slices.js';
import { flushEnvelopes, writeEnvelope } from './envelope.js';
import { handleHostResponse, installHostRpcTransport } from '../core/host-rpc.js';

/** 复用生成骨架，只把帧写出重定向到两条路径共用的 writeEnvelope。 */
class SharedWriteRuntime extends EndpointRuntime {
  write(frame: BusFrame): void {
    void writeEnvelope(frame);
  }
}

/**
 * 端点提供的能力集合 = 切片声明的完整 method 名。
 *
 * Hub 的能力表按 capability 精确匹配（hub.rs route_request: capability_index.get），
 * 宿主侧以完整 method（如 `compiler.dev`）发起请求，因此端点必须 announce 完整 method，
 * 而不能只声明命名空间前缀 —— 否则 Hub 路由不到，真实业务无法承接。
 */
export async function endpointCapabilities(): Promise<string[]> {
  const { plugins } = await loadSlices(resolveSliceSpec());
  const capabilities = new Set<string>(BRIDGE_METHODS);
  for (const plugin of plugins) {
    for (const method of plugin.methods) {
      capabilities.add(method);
    }
  }
  // 内建方法（bridge.ping/metrics/shutdown/heartbeat/plugins.resolve）由本进程的
  // handleBuiltinRequest 实现；Hub 单轨下必须一并 announce，否则这些方法路由不到，
  // handshake/metrics/shutdown 等辅助路径只能回落 1:1（违反单轨目标）。
  return [...capabilities].sort();
}

/**
 * 构造 Hub endpoint 的回调钩子。
 *
 * `publish` 由调用方注入（生产是 `runtime.publish`，测试是探针），
 * 让「插件事件 → Hub publish 帧」这条链可独立验证，不依赖真进程。
 */
export function makeHubHooks(publish: (event: BridgePluginEvent) => void): EndpointHooks {
  return {
    onRequest: async (capability, params, correlationId) => {
      // capability 即完整 method（如 `lint.run`），与 hub-stdio 的 announce 和
      // 宿主的 request.method 三方一致，无需再按命名空间二次推导。
      // params 是 Hub 帧的原样载荷，直接透传：不再做 `request.params ?? request`
      // 的启发式解包——业务参数自带 params 键时会丢掉 cwd 等兄弟字段。
      const exchange = await handleExchange(
        {
          id: correlationId || `hub-${Date.now()}`,
          method: capability,
          params: params as Record<string, unknown>,
        },
        // 插件在 handle 过程中实时 emit 的事件：立即 publish 到 Hub，
        // 宿主已订阅 event.*（provider.rs new()），可实时渲染进度/日志。
        async (event) => {
          publish(event);
        },
      );
      // 插件也可以把事件放在返回值里（旧行为）：同样 publish，两条来源都覆盖。
      for (const event of exchange.events) {
        publish({ method: event.method, params: event.params });
      }
      if (exchange.response.error) {
        return {
          error: {
            code: exchange.response.error.code,
            message: exchange.response.error.message,
          },
        };
      }
      return { result: exchange.response.result ?? null };
    },
    onEvent: () => {
      // host → 端点方向的事件目前无业务消费；端点 → host 的事件由上方 publish 承载。
    },
  };
}

/** 以 Hub endpoint 身份启动（announce → 收帧 → 响应）。 */
export async function runHubEndpoint(): Promise<void> {
  const provides = await endpointCapabilities();
  // 先建 runtime（含 publish 链），再建 hooks——onRequest 闭包持有 publish 回调。
  let publishSink: (event: BridgePluginEvent) => void = () => {};
  const hooks = makeHubHooks((event) => publishSink(event));

  // 与生成物 boot() 等价的生命周期，但写出走共享 writeEnvelope。
  const runtime = new SharedWriteRuntime(endpointIdFromEnv(), hooks);
  publishSink = (event) => runtime.publish(event.method, event.params as never);

  // Host RPC 反向通道（Node → Rust）：把 hostCall 的 envelope transport 换成
  // BusFrame request —— capability 即 host method（如 `host.fs.read`），由 Hub
  // 通配路由到宿主的 host.* 能力端点。响应回来后交回 hostCall 的 pending 机制。
  // 关键：runtime.request 的 pending 表无超时，response 丢失时永不 resolve。
  // 若直接 `await runtime.request`，hostCall 会卡在 transport.write，永远到不了
  // 它自己的 30s 超时消费点（`return await promise`）——形成永久挂起。
  // 这里用 race 加一层超时，超时/异常都主动回一个 host_response，
  // 让 hostCall 的 pending 一定被结清，与 1:1「write 立即返回」语义一致。
  const HOST_RPC_RESPONSE_TIMEOUT_MS = 30_000;
  installHostRpcTransport({
    write: (envelope) => {
      const { id, method, params } = envelope.payload;
      // write 立即返回 Promise.resolve()：真正的等待交给下面的异步链，
      // 避免阻塞 hostCall 的超时消费点。
      void (async () => {
        try {
          const result = await Promise.race([
            runtime.request(method, params as never),
            new Promise<never>((_, reject) => {
              const timer = setTimeout(
                () => reject(new Error('[E_HOST_RPC_TIMEOUT] host capability did not respond')),
                HOST_RPC_RESPONSE_TIMEOUT_MS,
              );
              timer.unref?.();
            }),
          ]);
          handleHostResponse({ id, result });
        } catch (err) {
          const detail = err as { code?: string; message?: string };
          handleHostResponse({
            id,
            error: {
              code: detail.code ?? 'E_HOST_RPC_TIMEOUT',
              message: detail.message ?? String(err),
            },
          });
        }
      })();
      return Promise.resolve();
    },
  });

  runtime.announce(provides, ['event.*']);
  const interval = Number(process.env.LANIA_BRIDGE_HEARTBEAT_MS ?? 0);
  if (interval > 0) {
    const timer = setInterval(() => runtime.heartbeat(), interval);
    (timer as NodeJS.Timeout).unref?.();
  }
  try {
    await runtime.run();
  } finally {
    runtime.bye();
    // 等共享写链排空，避免 stdio.ts 随后的 process.exit(0) 截断未写完的帧。
    await flushEnvelopes();
  }
}
