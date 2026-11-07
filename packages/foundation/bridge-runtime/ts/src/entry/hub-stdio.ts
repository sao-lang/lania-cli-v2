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

import { handleExchange } from './index.js';
import { loadSlices, resolveSliceSpec } from '../core/slices.js';
import { flushEnvelopes, writeEnvelope } from './envelope.js';

/** 复用生成骨架，只把帧写出重定向到两条路径共用的 writeEnvelope。 */
class SharedWriteRuntime extends EndpointRuntime {
  write(frame: BusFrame): void {
    void writeEnvelope(frame);
  }
}

/** 端点提供的能力集合 = 切片方法的命名空间前缀。 */
async function endpointCapabilities(): Promise<string[]> {
  const { plugins } = await loadSlices(resolveSliceSpec());
  const capabilities = new Set<string>();
  for (const plugin of plugins) {
    for (const method of plugin.methods) {
      const namespace = method.includes('.') ? method.split('.')[0] : method;
      capabilities.add(namespace);
    }
  }
  // 桥接自身的内建方法（ping/heartbeat/metrics 等）在 Hub 侧由宿主提供，
  // 端点只暴露插件能力，避免与宿主能力重名。
  return [...capabilities].sort();
}

/** 以 Hub endpoint 身份启动（announce → 收帧 → 响应）。 */
export async function runHubEndpoint(): Promise<void> {
  const provides = await endpointCapabilities();
  const hooks: EndpointHooks = {
    onRequest: async (capability, params) => {
      // Hub 的 capability 是命名空间（如 `lint`），映射到 bridge 的具体方法：
      // 先按命名空间找插件，再用 params.method（或唯一方法）确定具体方法。
      const request = params as Record<string, unknown>;
      const method =
        typeof request.method === 'string'
          ? request.method
          : `${capability}.${typeof request.action === 'string' ? request.action : ''}`;
      const exchange = await handleExchange({
        id: `hub-${Date.now()}`,
        method,
        params: (request.params as Record<string, unknown>) ?? request,
      });
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
      // 事件订阅：当前桥接的事件由 1:1 模式承载，Hub 模式只做透传占位。
    },
  };

  // 与生成物 boot() 等价的生命周期，但写出走共享 writeEnvelope。
  const runtime = new SharedWriteRuntime(endpointIdFromEnv(), hooks);
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
