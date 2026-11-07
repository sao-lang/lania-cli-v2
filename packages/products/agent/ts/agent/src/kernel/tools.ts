import { Type } from '@earendil-works/pi-ai';
import type { AgentTool } from '@earendil-works/pi-agent-core';

import type { HostCall } from '../types.js';
import { isRecord } from './guards.js';
import { runSubagent } from './subagent.js';

export interface ToolRuntime {
  hostCall: HostCall;
  cwd: string;
  sessionId: string;
  approvalMode: string;
  /** 子 agent 需要自己起 pi agent，因此要把解析好的模型带上。 */
  model?: import('@earendil-works/pi-ai').Model<any>;
  apiKey?: string;
  /** 0 = 主 agent；>0 表示子 agent，工具表里不再提供 `task`（结构性禁止递归）。 */
  depth?: number;
}

interface HostToolResult {
  status?: string;
  content?: string;
  details?: unknown;
  isError?: boolean;
}

function asParams(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

/**
 * 所有工具都只是宿主 RPC 的薄壳。
 *
 * 关键不变量：Node 侧不判断权限、不读文件、不执行命令。
 * `agent.tool.exec` 在 Rust 内完成「策略 → 审批 → 执行」，这里只转发参数。
 */
async function runHostTool(
  runtime: ToolRuntime,
  tool: string,
  params: Record<string, unknown>,
): Promise<{ content: { type: 'text'; text: string }[]; details: unknown; isError?: boolean }> {
  const exchange = await runtime.hostCall<HostToolResult>('agent.tool.exec', {
    cwd: runtime.cwd,
    sessionId: runtime.sessionId,
    tool,
    params,
    approvalMode: runtime.approvalMode,
  });
  const result = exchange.result ?? {};
  const text = typeof result.content === 'string' ? result.content : '';
  return {
    content: [{ type: 'text', text }],
    details: result.details ?? null,
    ...(result.isError === true || result.status === 'denied' ? { isError: true } : {}),
  };
}

export function createAgentTools(runtime: ToolRuntime): AgentTool<any>[] {
  return [
    {
      name: 'read',
      label: 'Read file',
      description:
        'Read a UTF-8 text file from the workspace. Optionally pass a 1-based line range via start/end. Output is truncated to 2000 lines.',
      parameters: Type.Object({
        path: Type.String({ description: 'Path relative to the workspace root' }),
        start: Type.Optional(Type.Number({ description: 'First line (1-based, inclusive)' })),
        end: Type.Optional(Type.Number({ description: 'Last line (1-based, inclusive)' })),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'read', asParams(params)),
    },
    {
      name: 'glob',
      label: 'Find files',
      description:
        'List files matching a glob pattern (e.g. "src/**/*.ts"). Honors .gitignore. Returns at most 500 paths.',
      parameters: Type.Object({
        pattern: Type.String({ description: 'Glob pattern' }),
        path: Type.Optional(Type.String({ description: 'Directory to search from' })),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'glob', asParams(params)),
    },
    {
      name: 'grep',
      label: 'Search content',
      description:
        'Search file contents with a Rust regex. Returns "path:line: text" lines, at most 200 matches.',
      parameters: Type.Object({
        pattern: Type.String({ description: 'Rust regex pattern' }),
        path: Type.Optional(Type.String({ description: 'Directory or file to search' })),
        glob: Type.Optional(Type.String({ description: 'Only search files matching this glob' })),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'grep', asParams(params)),
    },
    {
      name: 'write',
      label: 'Write file',
      description:
        'Create or overwrite a file with the given content. Prefer `edit` for small changes to existing files; use `write` for new files or full rewrites.',
      parameters: Type.Object({
        path: Type.String({ description: 'Path relative to the workspace root' }),
        content: Type.String({ description: 'Full file content' }),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'write', asParams(params)),
    },
    {
      name: 'edit',
      label: 'Edit file',
      description:
        'Replace an exact string in a file. old_string must occur exactly once unless replace_all is true. Prefer several small edits over one huge rewrite.',
      parameters: Type.Object({
        path: Type.String({ description: 'Path relative to the workspace root' }),
        old_string: Type.String({ description: 'Exact text to replace (include enough context)' }),
        new_string: Type.String({ description: 'Replacement text' }),
        replace_all: Type.Optional(Type.Boolean({ description: 'Replace every occurrence' })),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'edit', asParams(params)),
    },
    {
      name: 'exec',
      label: 'Run command',
      description:
        'Run a shell command in the workspace. Use it to build, test, or inspect the repository. Output is truncated to 20000 characters. Commands may require user approval.',
      parameters: Type.Object({
        command: Type.String({ description: 'Shell command line' }),
        timeout_ms: Type.Optional(Type.Number({ description: 'Timeout in milliseconds' })),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'exec', asParams(params)),
    },
    {
      name: 'git_status',
      label: 'Git status',
      description:
        'Show `git status --short --branch` for the workspace. Use it before summarizing what changed.',
      parameters: Type.Object({}),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'git_status', asParams(params)),
    },
    {
      name: 'git_diff',
      label: 'Git diff',
      description:
        'Show `git diff` (working tree by default, `--staged` for the index). Read-only; use it to review your own edits.',
      parameters: Type.Object({
        path: Type.Optional(Type.String({ description: 'Limit the diff to this path' })),
        staged: Type.Optional(
          Type.Boolean({ description: 'Diff the index instead of the worktree' }),
        ),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'git_diff', asParams(params)),
    },
    {
      name: 'project_command',
      label: 'Run lan command',
      description:
        'Run a lan CLI command (for example ["build"] or ["lint"]). Prefer this over exec when the project exposes a lan command for the job.',
      parameters: Type.Object({
        args: Type.Array(Type.String(), { description: 'Arguments after `lan`' }),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'project_command', asParams(params)),
    },
    // 子 agent 只在主 agent 上提供：深度 ≥ 1 时整条分支都不注册，从结构上禁止递归。
    ...((runtime.depth ?? 0) === 0
      ? [
          {
            name: 'task',
            label: 'Run sub-agent',
            description:
              'Delegate an investigation to a fresh sub-agent that has its own context window. Use it for broad searches or "read a lot, report little" work. read_only=false is serialized and its writes still need approval.',
            parameters: Type.Object({
              description: Type.String({ description: 'Short label for this sub-task' }),
              prompt: Type.String({ description: 'Self-contained instructions for the sub-agent' }),
              read_only: Type.Optional(
                Type.Boolean({
                  description: 'Restrict the sub-agent to read-tier tools (default true)',
                }),
              ),
            }),
            execute: async (_callId: string, params: unknown) => {
              const input = asParams(params);
              const result = await runSubagent(
                {
                  prompt: String(input.prompt ?? ''),
                  description:
                    typeof input.description === 'string' ? input.description : undefined,
                  readOnly: input.read_only !== false,
                },
                runtime,
              );
              return {
                content: [{ type: 'text' as const, text: result.text }],
                details: { toolCalls: result.toolCalls, stopReason: result.stopReason },
              };
            },
          },
        ]
      : []),
  ];
}
