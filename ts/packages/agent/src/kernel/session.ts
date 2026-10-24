import { Agent } from '@earendil-works/pi-agent-core';
import { getModels, getProviders, streamSimple } from '@earendil-works/pi-ai/compat';

import type { AgentPluginContext, AgentPluginResult, HostCall } from '../types.js';
import { isRecord } from './guards.js';
import { resolveModel } from './model.js';
import { createAgentTools } from './tools.js';

const DEFAULT_SYSTEM_PROMPT = [
  'You are `lan agent`, a coding agent running inside the lan CLI on a real workspace.',
  '- Inspect before you edit. Keep changes minimal and match the repository conventions.',
  '- Every tool runs in the host process and may require user approval.',
  '- If a tool call is denied, do not retry it blindly: explain what you need instead.',
  '- After changing code, verify with the project commands (`exec` or `project_command`).',
  '- Answer in the same language the user writes in.',
].join('\n');

interface SessionHandle {
  agent: Agent;
  cwd: string;
  modelLabel: string;
}

const sessions = new Map<string, SessionHandle>();

const THINKING_LEVELS = ['minimal', 'low', 'medium', 'high', 'xhigh', 'max'] as const;
type ThinkingLevelName = (typeof THINKING_LEVELS)[number];

function asThinkingLevel(value: unknown): ThinkingLevelName {
  return typeof value === 'string' && (THINKING_LEVELS as readonly string[]).includes(value)
    ? (value as ThinkingLevelName)
    : 'medium';
}

function sessionKey(params: Record<string, unknown>): string {
  const id = typeof params.sessionId === 'string' ? params.sessionId.trim() : '';
  if (!id) {
    throw new Error('agent session id is required');
  }
  return id;
}

function assistantText(message: unknown): string {
  if (!isRecord(message) || !Array.isArray(message.content)) {
    return '';
  }
  return message.content
    .map((block) =>
      isRecord(block) && block.type === 'text' && typeof block.text === 'string' ? block.text : '',
    )
    .join('');
}

function lastAssistant(messages: unknown[]): Record<string, unknown> | undefined {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (isRecord(message) && message.role === 'assistant') {
      return message;
    }
  }
  return undefined;
}

export async function createSession(
  cwd: string,
  params: Record<string, unknown>,
  hostCall: HostCall,
): Promise<Record<string, unknown>> {
  const sessionId = sessionKey(params);
  const provider = typeof params.provider === 'string' ? params.provider : undefined;
  const modelId = typeof params.model === 'string' ? params.model : undefined;
  const thinking = asThinkingLevel(params.thinking);
  const approvalMode = typeof params.approvalMode === 'string' ? params.approvalMode : 'always-ask';

  const { model, apiKey } = resolveModel(provider, modelId);
  const tools = createAgentTools({ hostCall, cwd, sessionId, approvalMode });

  const agent = new Agent({
    sessionId,
    initialState: {
      systemPrompt:
        typeof params.systemPrompt === 'string' && params.systemPrompt.trim().length > 0
          ? params.systemPrompt
          : DEFAULT_SYSTEM_PROMPT,
      model,
      thinkingLevel: thinking,
      tools,
      messages: [],
    },
    streamFn: (requestModel, context, options) =>
      streamSimple(requestModel, context, { ...options, apiKey }),
  });

  sessions.set(sessionId, { agent, cwd, modelLabel: `${model.provider}/${model.id}` });

  return {
    sessionId,
    model: `${model.provider}/${model.id}`,
    contextWindow: model.contextWindow,
    tools: tools.map((tool) => tool.name),
  };
}

export async function runPrompt(
  params: Record<string, unknown>,
  context: AgentPluginContext,
): Promise<AgentPluginResult> {
  const sessionId = sessionKey(params);
  const handle = sessions.get(sessionId);
  if (!handle) {
    throw new Error(`unknown agent session: ${sessionId}`);
  }
  const emit = context.emit;
  if (!emit) {
    throw new Error('agent prompt requires an event emitter');
  }
  const prompt = typeof params.prompt === 'string' ? params.prompt : '';
  if (!prompt.trim()) {
    throw new Error('prompt must not be empty');
  }

  const streamed = new Map<string, string>();
  const messageKey = (message: unknown, fallback: string): string => {
    const id = isRecord(message) ? message.id : undefined;
    return typeof id === 'string' && id.length > 0 ? id : fallback;
  };

  const unsubscribe = handle.agent.subscribe(async (event) => {
    switch (event.type) {
      case 'message_start': {
        streamed.set(messageKey(event.message, 'current'), '');
        break;
      }
      case 'message_update': {
        const key = messageKey(event.message, 'current');
        const text = assistantText(event.message);
        const previous = streamed.get(key) ?? '';
        if (text.length > previous.length && text.startsWith(previous)) {
          streamed.set(key, text);
          await emit({
            method: 'event.agent_text_delta',
            params: { sessionId, messageId: key, delta: text.slice(previous.length) },
          });
        }
        break;
      }
      case 'tool_execution_start': {
        await emit({
          method: 'event.agent_tool_start',
          params: {
            sessionId,
            toolCallId: event.toolCallId,
            tool: event.toolName,
            args: event.args,
          },
        });
        break;
      }
      case 'tool_execution_end': {
        await emit({
          method: 'event.agent_tool_end',
          params: {
            sessionId,
            toolCallId: event.toolCallId,
            tool: event.toolName,
            ok: !event.isError,
            durationMs: event.durationMs ?? null,
          },
        });
        break;
      }
      case 'agent_end': {
        const last = lastAssistant(event.messages);
        await emit({
          method: 'event.agent_end',
          params: { sessionId, usage: last?.usage ?? null, stopReason: last?.stopReason ?? null },
        });
        break;
      }
      default:
        break;
    }
  });

  try {
    await handle.agent.prompt(prompt);
  } finally {
    unsubscribe();
  }

  const messages: unknown[] = handle.agent.state.messages;
  const last = lastAssistant(messages);
  return {
    result: {
      sessionId,
      text: assistantText(last),
      usage: last?.usage ?? null,
      stopReason: last?.stopReason ?? null,
      messageCount: messages.length,
    },
    events: [],
  };
}

export function interruptSession(params: Record<string, unknown>): AgentPluginResult {
  const sessionId = sessionKey(params);
  const handle = sessions.get(sessionId);
  if (!handle) {
    throw new Error(`unknown agent session: ${sessionId}`);
  }
  handle.agent.abort();
  return { result: { ok: true }, events: [] };
}

export function listModels(): AgentPluginResult {
  const models: Array<Record<string, unknown>> = [];
  try {
    const providers = getProviders();
    for (const provider of providers ?? []) {
      let entries: unknown[] = [];
      try {
        entries = getModels(provider) ?? [];
      } catch {
        entries = [];
      }
      for (const entry of entries) {
        const model = isRecord(entry) ? entry : {};
        const id = typeof model.id === 'string' ? model.id : undefined;
        if (!id) {
          continue;
        }
        models.push({
          provider,
          id,
          contextWindow: typeof model.contextWindow === 'number' ? model.contextWindow : null,
        });
      }
    }
  } catch {
    // 目录不可用时返回空列表，由调用方决定如何提示。
  }
  return { result: { models }, events: [] };
}

export function disposeSession(params: Record<string, unknown>): AgentPluginResult {
  const sessionId = sessionKey(params);
  sessions.delete(sessionId);
  return { result: { ok: true }, events: [] };
}
