import { Type } from '@earendil-works/pi-ai';
import type { AgentTool } from '@earendil-works/pi-agent-core';

import type { HostCall } from '../types.js';
import { isRecord } from './guards.js';

export interface ToolRuntime {
  hostCall: HostCall;
  cwd: string;
  sessionId: string;
  approvalMode: string;
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
      name: 'project_command',
      label: 'Run lan command',
      description:
        'Run a lan CLI command (for example ["build"] or ["generate","module"]). Prefer this over exec when the project exposes a lan command for the job.',
      parameters: Type.Object({
        args: Type.Array(Type.String(), { description: 'Arguments after `lan`' }),
      }),
      execute: async (_callId: string, params: unknown) =>
        runHostTool(runtime, 'project_command', asParams(params)),
    },
  ];
}
