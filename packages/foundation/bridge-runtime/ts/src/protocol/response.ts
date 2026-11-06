/**
 * bridge 协议中的响应、错误与 exchange 模型定义。
 *
 * 主要导出：BridgeError、BridgeResponse、HandshakeResponse、BridgeExchange。
 */
import type { BridgeEvent } from './events.js';

export interface BridgeError {
  // 稳定的错误码（给 Rust 端映射 exit code / retry 策略用）。
  code: string;
  // 面向用户的简短错误信息。
  message: string;
  // 可选的结构化上下文（仅用于调试/观测，避免放入敏感信息）。
  data?: unknown;
}

export interface BridgeResponse<T = unknown> {
  // 请求的唯一标识，对应 BridgeRequest.id。
  id: string;
  // 成功结果（与 error 互斥；允许缺省用于仅发 events 的场景）。
  result?: T;
  // 协议级或插件运行时错误。
  error?: BridgeError;
}

export interface HandshakeResponse {
  // 协议版本：用于 Rust/TS 两端做兼容判断。
  protocolVersion: string;
  // bridge 名称：用于诊断与 metrics。
  bridgeName: string;
  // 支持的方法集合（包含内建 bridge 方法与插件方法）。
  methods: string[];
  // bridge 可能发出的事件集合。
  events: string[];
  // 期望的心跳间隔（stdio 侧定时发 event.heartbeat）。
  heartbeatIntervalMs: number;
  // 单个 request 允许积压的 events 上限（用于 backpressure 参考）。
  maxPendingEvents: number;
  // 失败策略：fail_fast 直接失败；reconnect 允许 Rust 端重启 bridge。
  failureStrategy: 'fail_fast' | 'reconnect';
}

export interface BridgeExchange<T = unknown> {
  // 最终响应。
  response: BridgeResponse<T>;
  // 与该请求相关的事件序列（日志/进度/产物/诊断等）。
  events: BridgeEvent[];
}
