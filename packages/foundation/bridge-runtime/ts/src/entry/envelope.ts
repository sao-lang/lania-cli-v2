/**
 * 唯一的 envelope 类型与帧写出实现（stdio.ts 的 1:1 路径与 hub-stdio.ts 的 Hub
 * 路径共同 import，决策 #12「协议逻辑不允许写两遍」）。
 *
 * 两套端点共享同一条 jsonl stdout 写链 `writeEnvelope`：
 * - 默认 1:1 模式写 `OutgoingEnvelope`（{ type, payload/requestId }）；
 * - `LANIA_BRIDGE_MODE=hub` 模式写 `@lania-cli-v2/bridge-protocol` 的 `BusFrame`。
 *
 * 生成物的 BusFrame 用 `kind` 判别、且没有 handshake 帧，与 1:1 的握手语义不兼容，
 * 因此不强行把 1:1 帧换成 BusFrame；而是把「唯一的 envelope 类型 + 唯一的
 * 序列化写出实现」抽到这里，让两条路径共用，删掉各自重复定义的部分。
 */
import type { BusFrame } from '@lania-cli-v2/bridge-protocol';

import type { BridgeRequest, HandshakeRequest } from '../protocol/request.js';

export type IncomingEnvelope =
  | { type: 'handshake'; payload: HandshakeRequest }
  | { type: 'request'; payload: BridgeRequest }
  | { type: 'host_response'; payload: unknown };

export type OutgoingEnvelope =
  | { type: 'handshake'; payload: unknown }
  | { type: 'event'; requestId: string; payload: unknown }
  | { type: 'response'; payload: unknown }
  | { type: 'host_request'; payload: unknown };

/** 两种端点路径共同写出的帧：1:1 envelope 或 Hub 的 BusFrame。 */
export type WireFrame = OutgoingEnvelope | BusFrame;

// 单一写链：保证 events 与 response、以及并发 handler 的帧不会交错。
let writeChain: Promise<void> = Promise.resolve();

/** 唯一的序列化 stdout 写出实现（jsonl 一行一帧）。 */
export function writeEnvelope(frame: WireFrame): Promise<void> {
  writeChain = writeChain.then(
    () =>
      new Promise<void>((resolve, reject) => {
        process.stdout.write(`${JSON.stringify(frame)}\n`, (error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      }),
  );
  return writeChain;
}

/** 唯一的入站帧解码（1:1 jsonl 一行）；Hub 路径用生成物的 decodeFrame。 */
export function parseEnvelope(line: string): IncomingEnvelope {
  return JSON.parse(line) as IncomingEnvelope;
}

/** 等待已排队的帧全部写出（Hub 端点退出前调用，避免被 process.exit 截断）。 */
export function flushEnvelopes(): Promise<void> {
  return writeChain;
}
