import { Agent } from '@earendil-works/pi-agent-core';
import { streamSimple } from '@earendil-works/pi-ai/compat';

import { createAgentTools, type ToolRuntime } from './tools.js';

/**
 * 子 agent：`task` 工具的实现。
 *
 * 与 omp 的差异（有意）：这里**没有**第二个执行通道。
 * 子 agent 是 Node 侧的又一个 pi `Agent` 实例，但它调用的每个工具
 * 仍然走 `agent.tool.exec` → 宿主的策略与审批。所以「执行权威在 Rust」没有被绕过。
 *
 * 并发模型：
 * - 同时最多 4 个子 agent（`MAX_CONCURRENT`），超出的排队；
 * - 只读子 agent 可以并行跑；
 * - **可写**子 agent 串行执行（`writeChain`），避免两个 agent 同时改同一片文件。
 *
 * 深度：子 agent 的工具表里不包含 `task`，从结构上杜绝递归。
 */

export interface SubagentRequest {
  prompt: string;
  description?: string;
  readOnly: boolean;
}

export interface SubagentResult {
  text: string;
  toolCalls: number;
  stopReason?: string;
}

/** 只读子 agent 可用的工具（与宿主 read tier 对齐）。 */
const READ_ONLY_TOOLS = new Set(['read', 'glob', 'grep', 'git_status', 'git_diff']);

const MAX_CONCURRENT = 4;
const MAX_RESULT_CHARS = 8_000;

let active = 0;
const waiting: Array<() => void> = [];

async function acquire(): Promise<void> {
  if (active < MAX_CONCURRENT) {
    active += 1;
    return;
  }
  await new Promise<void>((resolve) => waiting.push(resolve));
  active += 1;
}

function release(): void {
  active -= 1;
  const next = waiting.shift();
  if (next) {
    next();
  }
}

/** 可写子 agent 的全局串行锁：保证同一时刻只有一个会写盘的 agent 在跑。 */
let writeChain: Promise<unknown> = Promise.resolve();

function withWriteLock<T>(task: () => Promise<T>): Promise<T> {
  const run = writeChain.then(task, task);
  writeChain = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

const SUBAGENT_PROMPT = [
  'You are a focused sub-agent inside `lan pi`.',
  '- You cannot ask the user questions: investigate and answer on your own.',
  '- Prefer read-only tools; report file paths and line numbers you actually saw.',
  '- Finish with a compact answer (what you found, where, and what you did not verify).',
].join('\n');

export async function runSubagent(
  request: SubagentRequest,
  runtime: ToolRuntime,
): Promise<SubagentResult> {
  if (!runtime.model) {
    throw new Error('subagent requires a resolved model');
  }
  const prompt = request.prompt?.trim();
  if (!prompt) {
    throw new Error('subagent prompt must not be empty');
  }

  const run = async (): Promise<SubagentResult> => {
    await acquire();
    try {
      const tools = createAgentTools({ ...runtime, depth: 1 }).filter((tool) =>
        request.readOnly ? READ_ONLY_TOOLS.has(tool.name) : tool.name !== 'task',
      );

      let toolCalls = 0;
      const agent = new Agent({
        initialState: {
          systemPrompt: `${SUBAGENT_PROMPT}\n\nYour assignment: ${
            request.description?.trim() || 'complete the task'
          }`,
          model: runtime.model,
          tools,
        },
        streamFn: (model, context, options) =>
          streamSimple(model, context, { ...options, apiKey: runtime.apiKey }),
      });
      agent.subscribe((event) => {
        if (event.type === 'tool_execution_start') {
          toolCalls += 1;
        }
      });

      await agent.prompt(prompt);
      const messages = agent.state.messages;
      const last = [...messages].reverse().find((message) => message.role === 'assistant');
      const text = extractText(last).slice(0, MAX_RESULT_CHARS);
      return {
        text: text || '(sub-agent returned no text)',
        toolCalls,
        stopReason: last && 'stopReason' in last ? String(last.stopReason) : undefined,
      };
    } finally {
      release();
    }
  };

  return request.readOnly ? run() : withWriteLock(run);
}

function extractText(message: unknown): string {
  if (!message || typeof message !== 'object') {
    return '';
  }
  const content = (message as { content?: unknown }).content;
  if (typeof content === 'string') {
    return content;
  }
  if (!Array.isArray(content)) {
    return '';
  }
  return content
    .map((block) =>
      block && typeof block === 'object' && (block as { type?: string }).type === 'text'
        ? String((block as { text?: unknown }).text ?? '')
        : '',
    )
    .join('');
}
