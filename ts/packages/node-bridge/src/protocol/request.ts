/**
 * bridge 协议中的请求与握手模型定义。
 *
 * 主要导出：BridgeRequest、HandshakeRequest。
 */
export interface BridgeRequest<T = unknown> {
  id: string;
  method: string;
  params: T;
}

export interface HandshakeRequest {
  protocolVersion: string;
  transport: 'stdio';
  encoding: 'json';
  hostName: string;
}
