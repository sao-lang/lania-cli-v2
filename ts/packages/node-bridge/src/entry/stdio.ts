/**
 * stdio 协议入口，负责收发 envelope、握手、事件流与心跳。
 *
 * 关键点：
 * - 包含 stdio 协议/流式读写
 * - 包含 JSON 协议/序列化
 *
 * 传输约定（jsonl）：
 * - 每一行 stdout 是一个 JSON envelope：`{ type, payload, ... }`
 * - 输入同理（stdin jsonl），Rust 端与 Node 端都按“逐行一条消息”解析
 *
 * 非常重要：
 * - stdout 必须只输出协议 envelope。任何 console.log 都会污染协议并导致上游解析失败。
 * - events 与 response 的顺序必须稳定：同一个 request 的 events 必须先于最终 response 发出。
 *
 * requestId 约定：
 * - 普通请求事件：requestId = BridgeRequest.id
 * - 全局事件（与具体请求无关，如 ready/heartbeat）：requestId 固定为 `bridge`
 */
import readline from 'node:readline';

import { createHandshakeResponse, handleExchange, readyEvent } from './index.js';
import { handleHostResponse, installHostRpcTransport } from '../core/host-rpc.js';
import type { BridgeRequest, HandshakeRequest } from '../protocol/request.js';

type IncomingEnvelope =
  | { type: 'handshake'; payload: HandshakeRequest }
  | { type: 'request'; payload: BridgeRequest }
  | { type: 'host_response'; payload: unknown };

type OutgoingEnvelope =
  | { type: 'handshake'; payload: unknown }
  | { type: 'event'; requestId: string; payload: unknown }
  | { type: 'response'; payload: unknown }
  | { type: 'host_request'; payload: unknown };

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

let heartbeatTimer: NodeJS.Timeout | null = null;
let writeChain: Promise<void> = Promise.resolve();

installHostRpcTransport({
  write: async (envelope) => {
    await writeEnvelope(envelope);
  },
});

// bridge 启动后立即发出 ready 事件：
// - requestId 固定为 `bridge`，表示“与具体请求无关的全局事件”。
// - Rust 端可以据此确认 bridge 进程已可工作，并显示已加载插件列表。
await writeEnvelope({
  type: 'event',
  requestId: 'bridge',
  payload: readyEvent(),
});

for await (const line of rl) {
  const trimmed = line.trim();
  if (!trimmed) {
    continue;
  }

  // 注意：stdio 传输严格要求 stdout 只输出协议 envelope（jsonl）。
  // bridge 内部如果有多余 console.log/println，会污染协议并导致 Rust 端解析失败。
  const envelope = JSON.parse(trimmed) as IncomingEnvelope;
  if (envelope.type === 'handshake') {
    const handshake = createHandshakeResponse(envelope.payload);
    await writeEnvelope({
      type: 'handshake',
      payload: handshake,
    });
    if (!heartbeatTimer) {
      // 通过定时 event.heartbeat 保持链路活跃，并让 Rust 端检测“假死”。
      // unref()：不阻止进程退出；真正的生命周期由 stdin 是否关闭决定（父进程退出/管道关闭时 for-await 会结束）。
      heartbeatTimer = setInterval(async () => {
        await writeEnvelope({
          type: 'event',
          requestId: 'bridge',
          payload: {
            method: 'event.heartbeat',
            params: {
              ts: Date.now(),
            },
          },
        });
      }, handshake.heartbeatIntervalMs).unref();
    }
    continue;
  }

  if (envelope.type === 'request') {
    // Do not block stdin reader: host RPC responses may arrive while we are handling a request.
    void (async () => {
      const exchange = await handleExchange(envelope.payload);
      // 协议约定：同一个 request 可能产生多个 events，最后以单个 response 结束。
      for (const event of exchange.events) {
        await writeEnvelope({
          type: 'event',
          requestId: envelope.payload.id,
          payload: event,
        });
      }
      // response 一定要在最后写出：Rust 端会把它视为“请求完成信号”并结束等待。
      await writeEnvelope({
        type: 'response',
        payload: exchange.response,
      });
    })();
    continue;
  }

  if (envelope.type === 'host_response') {
    handleHostResponse(envelope.payload as any);
  }
}

async function writeEnvelope(envelope: OutgoingEnvelope): Promise<void> {
  // Serialize writes so concurrent handlers won't interleave envelopes.
  writeChain = writeChain.then(
    () =>
      new Promise<void>((resolve, reject) => {
        process.stdout.write(`${JSON.stringify(envelope)}\n`, (error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      }),
  );
  await writeChain;
}
