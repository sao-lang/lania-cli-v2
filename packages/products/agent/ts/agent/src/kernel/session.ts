import { Agent } from '@earendil-works/pi-agent-core';
import type { AgentMessage } from '@earendil-works/pi-agent-core';
import { getModels, getProviders, streamSimple } from '@earendil-works/pi-ai/compat';

import type { AgentPluginContext, AgentPluginResult, HostCall } from '../types.js';
import { isRecord } from './guards.js';
import { resolveModel } from './model.js';
import { createAgentTools } from './tools.js';

/**
 * 这个文件是「模型编排」的全部：建 agent、跑一轮、把 pi 的事件翻译成 bridge 事件。
 *
 * 三条不变量：
 * 1. 不落盘：历史由 Rust 通过 `agent.session.create.messages` 传入，Node 只持有内存态；
 * 2. 不判权限：工具执行一律 `hostCall('agent.tool.exec')`，审批在 Rust；
 * 3. 适配而非镜像：pi 的事件语义在 `translateEvent()` 里收敛，
 *    其余文件不应该出现 `agent.subscribe`。
 */

const DEFAULT_SYSTEM_PROMPT = [
  'You are `lan pi`, a coding agent running inside the lan CLI on a real workspace.',
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
  contextWindow: number | null;
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

function requireHandle(params: Record<string, unknown>): SessionHandle {
  const sessionId = sessionKey(params);
  const handle = sessions.get(sessionId);
  if (!handle) {
    throw new Error(`unknown agent session: ${sessionId}`);
  }
  return handle;
}

/** 把 content 数组里某一类块按顺序拼起来。 */
function contentText(message: unknown, blockType: 'text' | 'thinking'): string {
  if (!isRecord(message) || !Array.isArray(message.content)) {
    return '';
  }
  return message.content
    .map((block) =>
      isRecord(block) && block.type === blockType
        ? typeof block[blockType === 'text' ? 'text' : 'thinking'] === 'string'
          ? String(block[blockType === 'text' ? 'text' : 'thinking'])
          : ''
        : '',
    )
    .join('');
}

function assistantText(message: unknown): string {
  return contentText(message, 'text');
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

/** 历史回放：Rust 传进来的就是 pi 自己的消息形状，这里只做基本校验。 */
function restoreMessages(value: unknown): AgentMessage[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter(
    (message) => isRecord(message) && typeof message.role === 'string',
  ) as AgentMessage[];
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
  const tools = createAgentTools({
    hostCall,
    cwd,
    sessionId,
    approvalMode,
    model,
    apiKey,
    depth: 0,
  });
  const messages = restoreMessages(params.messages);

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
      // 空数组会被 pi 当成「没有历史」；只有恢复时才传 messages。
      ...(messages.length > 0 ? { messages } : {}),
    },
    streamFn: (requestModel, context, options) =>
      streamSimple(requestModel, context, { ...options, apiKey }),
  });

  sessions.set(sessionId, {
    agent,
    cwd,
    modelLabel: `${model.provider}/${model.id}`,
    contextWindow: typeof model.contextWindow === 'number' ? model.contextWindow : null,
  });

  return {
    sessionId,
    model: `${model.provider}/${model.id}`,
    contextWindow: model.contextWindow ?? null,
    restoredMessages: messages.length,
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

  // 记录起点：本轮新增的消息要原样回传给 Rust 落盘。
  const startIndex = handle.agent.state.messages.length;
  const toolsUsed: string[] = [];

  const unsubscribe = handle.agent.subscribe(async (event) => {
    switch (event.type) {
      case 'agent_start': {
        await emit({
          method: 'event.agent_start',
          params: { sessionId, model: handle.modelLabel },
        });
        break;
      }
      case 'message_update': {
        for (const delta of streamDeltas(handle, event.message)) {
          await emit({ method: delta.method, params: { sessionId, ...delta.params } });
        }
        break;
      }
      case 'tool_execution_start': {
        if (!toolsUsed.includes(event.toolName)) {
          toolsUsed.push(event.toolName);
        }
        await emit({
          method: 'event.agent_tool_start',
          params: {
            sessionId,
            toolCallId: event.toolCallId,
            tool: event.toolName,
            params: event.args ?? null,
          },
        });
        break;
      }
      case 'tool_execution_update': {
        await emit({
          method: 'event.agent_tool_update',
          params: {
            sessionId,
            toolCallId: event.toolCallId,
            partial: event.partialResult ?? null,
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
            summary: summarizeToolResult(event.result, event.isError),
          },
        });
        break;
      }
      case 'turn_end': {
        const usage = isRecord(event.message) ? event.message.usage : undefined;
        await emit({
          method: 'event.agent_turn_end',
          params: { sessionId, usage: usage ?? null },
        });
        break;
      }
      case 'agent_end': {
        const last = lastAssistant(event.messages);
        await emit({
          method: 'event.agent_end',
          params: {
            sessionId,
            usage: last?.usage ?? null,
            stopReason: last?.stopReason ?? null,
          },
        });
        break;
      }
      default:
        break;
    }
  });

  try {
    await handle.agent.prompt(prompt);
  } catch (error) {
    await emit({
      method: 'event.agent_error',
      params: {
        sessionId,
        code: 'E_AGENT_RUN',
        message: error instanceof Error ? error.message : String(error),
      },
    });
    throw error;
  } finally {
    unsubscribe();
  }

  const messages: unknown[] = handle.agent.state.messages;
  // 只回传「模型产出」的消息：user 消息由 Rust 在发问前就已落盘，
  // 这里再带回去会导致会话文件里出现两条重复的 user 记录。
  const added = messages
    .slice(startIndex)
    .filter((message) => !(isRecord(message) && message.role === 'user'));
  const last = lastAssistant(messages);
  return {
    result: {
      sessionId,
      text: assistantText(last),
      usage: last?.usage ?? null,
      stopReason: last?.stopReason ?? null,
      messageCount: messages.length,
      // 只回传本轮新增的消息：Rust 把它们 append 到 jsonl。
      messages: added,
      toolsUsed,
    },
    events: [],
  };
}

/**
 * 增量文本/思考：pi 给的是「整条消息的最新内容」，
 * 因此这里 diff 出增量，避免把全文重复推给 Rust。
 */
const streamState = new WeakMap<SessionHandle, Map<string, { text: string; thinking: string }>>();

function streamDeltas(
  handle: SessionHandle,
  message: unknown,
): Array<{ method: string; params: Record<string, unknown> }> {
  let perSession = streamState.get(handle);
  if (!perSession) {
    perSession = new Map();
    streamState.set(handle, perSession);
  }
  const id =
    isRecord(message) && typeof message.id === 'string' && message.id.length > 0
      ? message.id
      : 'current';
  const previous = perSession.get(id) ?? { text: '', thinking: '' };
  const text = assistantText(message);
  const thinking = contentText(message, 'thinking');
  perSession.set(id, { text, thinking });

  const deltas: Array<{ method: string; params: Record<string, unknown> }> = [];
  if (text.length > previous.text.length && text.startsWith(previous.text)) {
    deltas.push({
      method: 'event.agent_text_delta',
      params: { messageId: id, delta: text.slice(previous.text.length) },
    });
  }
  if (thinking.length > previous.thinking.length && thinking.startsWith(previous.thinking)) {
    deltas.push({
      method: 'event.agent_thinking_delta',
      params: { messageId: id, delta: thinking.slice(previous.thinking.length) },
    });
  }
  return deltas;
}

/** 工具结果的单行摘要（给 TUI 的工具卡片用，不参与上下文）。 */
function summarizeToolResult(result: unknown, isError: boolean): string | null {
  if (isError) {
    return 'failed';
  }
  const details = isRecord(result) ? result.details : undefined;
  if (isRecord(details)) {
    const keys = ['count', 'lines', 'exitCode', 'replacements'];
    const parts = keys
      .filter((key) => typeof details[key] === 'number')
      .map((key) => `${key}=${details[key]}`);
    if (parts.length > 0) {
      return parts.join(' ');
    }
  }
  if (isRecord(result) && Array.isArray(result.content)) {
    const text = result.content
      .map((block) => (isRecord(block) && typeof block.text === 'string' ? block.text : ''))
      .join('');
    const lines = text.split('\n').filter(Boolean).length;
    if (lines > 0) {
      return `${lines} lines`;
    }
  }
  return null;
}

export function sessionState(params: Record<string, unknown>): AgentPluginResult {
  const handle = requireHandle(params);
  const state = handle.agent.state;
  return {
    result: {
      isStreaming: Boolean(state.isStreaming),
      pendingToolCalls: Array.isArray(state.pendingToolCalls) ? state.pendingToolCalls.length : 0,
      messageCount: Array.isArray(state.messages) ? state.messages.length : 0,
      contextWindow: handle.contextWindow,
      error: state.errorMessage ?? null,
    },
    events: [],
  };
}

export function interruptSession(params: Record<string, unknown>): AgentPluginResult {
  const handle = requireHandle(params);
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
          reasoning: model.reasoning === true,
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
  const handle = sessions.get(sessionId);
  if (handle) {
    streamState.delete(handle);
  }
  sessions.delete(sessionId);
  return { result: { ok: true }, events: [] };
}
