import type { BridgeEvent } from '../protocol/events.js';

export const BRIDGE_NAME = '@lania-cli/node-bridge';
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
  'event.shutdown',
  'event.heartbeat',
];

export const BRIDGE_HEARTBEAT_INTERVAL_MS = 10_000;
export const BRIDGE_MAX_PENDING_EVENTS = 32;
export const BRIDGE_FAILURE_STRATEGY = 'reconnect' as const;
