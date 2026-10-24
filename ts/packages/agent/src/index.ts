/**
 * `@lania-cli/agent`：lania 的 agent 内核。
 *
 * 设计边界：
 * - 本包只做「模型编排」：pi 的 agent loop、provider、工具 schema 适配。
 * - 一切 I/O、策略、审批、执行都在 Rust 宿主（`agent.tool.exec`）。
 * - 因此这里禁止 import `fs` / `node:fs` / `child_process` / `net`。
 */
export { agentPlugin, AGENT_METHODS } from './plugin.js';
export type { AgentPluginContext, AgentPluginResult, EmitEvent, HostCall } from './types.js';
