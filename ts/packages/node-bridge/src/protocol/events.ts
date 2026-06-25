/**
 * bridge 协议中的事件模型定义。
 *
 * 主要导出：BridgeEventMethod、BridgeEvent。
 */
export type BridgeEventMethod =
  | 'event.ready'
  | 'event.log'
  | 'event.progress'
  | 'event.dev_url'
  | 'event.build_asset'
  | 'event.compiler_start'
  | 'event.compiler_status'
  | 'event.compiler_server_ready'
  | 'event.compiler_asset'
  | 'event.compiler_issue'
  | 'event.compiler_watch_change'
  | 'event.compiler_done'
  | 'event.lint_start'
  | 'event.lint_file'
  | 'event.lint_result'
  | 'event.lint_summary'
  | 'event.watch_change'
  | 'event.shutdown'
  | 'event.heartbeat';

export interface BridgeEvent<T = unknown> {
  method: BridgeEventMethod;
  params: T;
}
