import type { BridgeEvent } from '../protocol/events.js';

export const BRIDGE_NAME = '@lania-cli-v2/node-bridge';
export const BRIDGE_METHODS = [
  'bridge.ping',
  'bridge.shutdown',
  'bridge.metrics',
  'bridge.subscribe',
  'bridge.heartbeat',
  'plugins.resolve',
] as const;

export const BRIDGE_EVENTS: BridgeEvent['method'][] = [
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
  'event.agent_text_delta',
  'event.agent_tool_start',
  'event.agent_tool_end',
  'event.agent_end',
  'event.shutdown',
  'event.heartbeat',
];

export const BRIDGE_HEARTBEAT_INTERVAL_MS = 10_000;
export const BRIDGE_MAX_PENDING_EVENTS = 32;
export const BRIDGE_FAILURE_STRATEGY = 'reconnect' as const;
