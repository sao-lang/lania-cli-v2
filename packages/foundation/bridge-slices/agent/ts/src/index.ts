/**
 * agent 切片：把 `@lania-cli-v2/agent`（pi 引擎）适配为 BridgePlugin。
 *
 * 主要导出：plugin。
 *
 * 原实现位于核 plugin-registry.ts 的 agentBridgePlugin 边界适配（阶段 4b 迁入切片）：
 * agent 用结构化事件类型声明，流式输出全部走 `emit`，返回值里不带事件，
 * 因此这里做一次运行时边界适配，避免 bridge 与 agent 互相依赖。
 */
import type { BridgePlugin } from '@lania-cli-v2/bridge-runtime/core/bridge-plugin.js';
import type { BridgeEvent } from '@lania-cli-v2/bridge-runtime/protocol/events.js';
import { agentPlugin } from '@lania-cli-v2/agent';

type MinimalAgentPlugin = {
  name: string;
  methods: readonly string[];
  handle: BridgePlugin['handle'];
};

function adaptAgentPlugin(agentPlugin: MinimalAgentPlugin): BridgePlugin {
  return {
    name: agentPlugin.name,
    methods: [...agentPlugin.methods],
    handle: async (method, params, context) => {
      const result = await agentPlugin.handle(method, params, {
        cwd: context?.cwd ?? null,
        ...(context?.emit ? { emit: context.emit } : {}),
        ...(context?.hostCall ? { hostCall: context.hostCall } : {}),
      });
      if (!result) {
        return undefined;
      }
      return { result: result.result, events: [] as BridgeEvent[] };
    },
  };
}

export const plugin: BridgePlugin = adaptAgentPlugin(agentPlugin as unknown as MinimalAgentPlugin);
