import path from 'node:path';

import { asRecord, loadLanConfig } from '../core/runtime.js';
import type { SchemaToolContext } from './types.js';

export type ToolsPolicyResolved = {
  allow: string[] | null;
  deny: Set<string>;
  exec: { allowShell: boolean; allowEnvWrite: boolean };
  fs: { writeRoot: string };
  bridge: { allowMethods: string[] | null };
};

export interface ToolsPolicyManager {
  get: () => Promise<ToolsPolicyResolved>;
  assertToolAllowed: (tool: string, methodName: string) => Promise<void>;
  assertBridgeMethodAllowed: (bridgeMethod: string) => Promise<void>;
  assertExecAllowed: (
    operation: string,
    options?: { useShell?: boolean; env?: Record<string, string> },
  ) => Promise<void>;
  assertFsWriteAllowed: (operation: string, filePath: string) => Promise<void>;
  assertHostCallAllowed: (method: string) => Promise<void>;
  assertInteractionAllowed: (operation: string) => Promise<void>;
  assertGitAllowed: (operation: string) => Promise<void>;
  assertPmAllowed: (operation: string) => Promise<void>;
  assertLogAllowed: (operation: string) => Promise<void>;
  assertTasksAllowed: (operation: string) => Promise<void>;
  assertProgressAllowed: (operation: string) => Promise<void>;
  assertConfigAllowed: (operation: string) => Promise<void>;
  assertWorkspaceAllowed: (operation: string) => Promise<void>;
  assertJsonAllowed: (operation: string) => Promise<void>;
}

function resolveToolsPolicy(raw: unknown): ToolsPolicyResolved {
  const record = asRecord(raw);
  const allow = Array.isArray(record.allow)
    ? record.allow.filter((item): item is string => typeof item === 'string')
    : null;
  const deny = new Set(
    Array.isArray(record.deny)
      ? record.deny.filter((item): item is string => typeof item === 'string')
      : [],
  );
  const execRecord = asRecord(record.exec);
  const fsRecord = asRecord(record.fs);
  const bridgeRecord = asRecord(record.bridge);
  const allowMethods = Array.isArray(bridgeRecord.allowMethods)
    ? bridgeRecord.allowMethods.filter((item): item is string => typeof item === 'string')
    : null;
  return {
    allow: allow && allow.length > 0 ? allow : null,
    deny,
    exec: {
      allowShell: execRecord.allowShell !== false,
      allowEnvWrite: execRecord.allowEnvWrite !== false,
    },
    fs: {
      writeRoot:
        typeof fsRecord.writeRoot === 'string' && fsRecord.writeRoot.trim()
          ? fsRecord.writeRoot
          : '.',
    },
    bridge: {
      allowMethods: allowMethods && allowMethods.length > 0 ? allowMethods : null,
    },
  };
}

function globToRegExp(pattern: string): RegExp {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
  return new RegExp(`^${escaped}$`);
}

function methodAllowedByPatterns(method: string, patterns: string[] | null): boolean {
  if (!patterns) return true;
  return patterns.some((pattern) => globToRegExp(pattern).test(method));
}

function resolvePathUnderRoot(cwd: string, root: string, targetPath: string): boolean {
  const rootAbs = path.resolve(cwd, root);
  const targetAbs = path.isAbsolute(targetPath)
    ? path.resolve(targetPath)
    : path.resolve(cwd, targetPath);
  const rel = path.relative(rootAbs, targetAbs);
  return rel === '' || (!rel.startsWith('..' + path.sep) && rel !== '..' && !path.isAbsolute(rel));
}

function parseHostToolFromMethod(method: string): { tool: string; method: string } {
  if (!method.startsWith('host.')) {
    return { tool: 'host', method };
  }
  const rest = method.slice('host.'.length);
  const [tool, ...parts] = rest.split('.');
  return { tool: tool || 'host', method: parts.join('.') || tool || 'host' };
}

function toolsPolicyError(message: string): Error {
  return new Error(`[E_TOOLS_DENIED] ${message}`);
}

export function createToolsPolicyManager(base: SchemaToolContext): ToolsPolicyManager {
  let cached: Promise<ToolsPolicyResolved> | null = null;
  const get = async (): Promise<ToolsPolicyResolved> => {
    if (!cached) {
      cached = (async () => {
        const loaded = await loadLanConfig(base.cwd);
        const config = asRecord(loaded.config);
        const tools = (config.tools ?? (config as any).tooling) as unknown;
        return resolveToolsPolicy(tools);
      })();
    }
    return cached;
  };

  const assertToolAllowed = async (tool: string, methodName: string) => {
    const policy = await get();
    if (policy.deny.has(tool)) {
      throw toolsPolicyError(`tools.${tool}.${methodName} is denied by config.tools.deny`);
    }
    if (policy.allow && !policy.allow.includes(tool)) {
      throw toolsPolicyError(`tools.${tool}.${methodName} is not in config.tools.allow`);
    }
  };

  return {
    get,
    assertToolAllowed,
    assertBridgeMethodAllowed: async (bridgeMethod: string) => {
      await assertToolAllowed('bridge', 'raw.call');
      const policy = await get();
      if (!methodAllowedByPatterns(bridgeMethod, policy.bridge.allowMethods)) {
        throw toolsPolicyError(
          `tools.bridge method ${bridgeMethod} is blocked by config.tools.bridge.allowMethods`,
        );
      }
    },
    assertExecAllowed: async (
      operation: string,
      options?: { useShell?: boolean; env?: Record<string, string> },
    ) => {
      await assertToolAllowed('exec', operation);
      const policy = await get();
      if (options?.useShell && !policy.exec.allowShell) {
        throw toolsPolicyError(
          `tools.exec.${operation} is blocked (config.tools.exec.allowShell=false)`,
        );
      }
      const envKeys = options?.env ? Object.keys(options.env) : [];
      if (envKeys.length > 0 && !policy.exec.allowEnvWrite) {
        throw toolsPolicyError(
          `tools.exec.${operation} is blocked (config.tools.exec.allowEnvWrite=false)`,
        );
      }
    },
    assertFsWriteAllowed: async (operation: string, filePath: string) => {
      await assertToolAllowed('fs', operation);
      const policy = await get();
      if (!resolvePathUnderRoot(base.cwd, policy.fs.writeRoot, filePath)) {
        throw toolsPolicyError(
          `tools.fs.${operation} is blocked: path is outside writeRoot (${policy.fs.writeRoot})`,
        );
      }
    },
    assertHostCallAllowed: async (method: string) => {
      const { tool, method: toolMethod } = parseHostToolFromMethod(method);
      await assertToolAllowed(tool, toolMethod || 'call');
    },
    assertInteractionAllowed: async (operation: string) => {
      await assertToolAllowed('interaction', operation);
    },
    assertGitAllowed: async (operation: string) => assertToolAllowed('git', operation),
    assertPmAllowed: async (operation: string) => assertToolAllowed('pm', operation),
    assertLogAllowed: async (operation: string) => assertToolAllowed('log', operation),
    assertTasksAllowed: async (operation: string) => assertToolAllowed('tasks', operation),
    assertProgressAllowed: async (operation: string) => assertToolAllowed('progress', operation),
    assertConfigAllowed: async (operation: string) => assertToolAllowed('config', operation),
    assertWorkspaceAllowed: async (operation: string) => assertToolAllowed('workspace', operation),
    assertJsonAllowed: async (operation: string) => assertToolAllowed('json', operation),
  };
}
