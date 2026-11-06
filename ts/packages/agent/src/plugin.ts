import type { AgentPlugin, AgentPluginContext, AgentPluginResult } from './types.js';
import {
  createSession,
  disposeSession,
  interruptSession,
  listModels,
  runPrompt,
  sessionState,
} from './kernel/session.js';

/**
 * bridge 方法表，与 Rust 侧 `lania-agent-core::bridge` 一一对应（改动需同 PR）。
 *
 * - `agent.session.create`：历史由 Rust 传入（含 `messages` 恢复）
 * - `agent.prompt`：流式事件 + 本轮新增消息
 * - `agent.interrupt`：只中断生成，不销毁会话
 * - `agent.state`：运行态查询
 * - `agent.models.list`：模型目录
 * - `agent.session.dispose`：释放内存态
 */
export const AGENT_METHODS = [
  'agent.session.create',
  'agent.prompt',
  'agent.interrupt',
  'agent.state',
  'agent.models.list',
  'agent.session.dispose',
];

export const agentPlugin: AgentPlugin = {
  name: 'agent',
  methods: AGENT_METHODS,

  async handle(
    method: string,
    params: Record<string, unknown>,
    context?: AgentPluginContext,
  ): Promise<AgentPluginResult | undefined> {
    switch (method) {
      case 'agent.session.create': {
        if (!context?.hostCall) {
          throw new Error('agent.session.create requires a host bridge context');
        }
        return {
          result: await createSession(context.cwd ?? process.cwd(), params, context.hostCall),
          events: [],
        };
      }
      case 'agent.prompt': {
        if (!context) {
          throw new Error('agent.prompt requires a host bridge context');
        }
        return runPrompt(params, context);
      }
      case 'agent.interrupt':
        return interruptSession(params);
      case 'agent.state':
        return sessionState(params);
      case 'agent.session.dispose':
        return disposeSession(params);
      case 'agent.models.list':
        return listModels();
      default:
        return undefined;
    }
  },
};
