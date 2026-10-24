import type { AgentPlugin, AgentPluginContext, AgentPluginResult } from './types.js';
import {
  createSession,
  disposeSession,
  interruptSession,
  listModels,
  runPrompt,
} from './kernel/session.js';

export const AGENT_METHODS = [
  'agent.session.create',
  'agent.prompt',
  'agent.interrupt',
  'agent.session.dispose',
  'agent.models.list',
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
      case 'agent.session.dispose':
        return disposeSession(params);
      case 'agent.models.list':
        return listModels();
      default:
        return undefined;
    }
  },
};
