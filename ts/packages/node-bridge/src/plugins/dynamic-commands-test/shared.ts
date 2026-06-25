import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  handleHostResponse,
  installHostRpcTransport,
  resetHostRpcTransport,
} from '../../core/host-rpc.js';
import { handleExchange } from '../../entry/index.js';

export {
  assert,
  handleExchange,
  handleHostResponse,
  installHostRpcTransport,
  readFile,
  readdir,
  resetHostRpcTransport,
  rm,
  stat,
  writeFile,
};

// Shared fixture helpers stay in one place so split test modules can build the same
// temporary runtime-command project shape without duplicating setup logic.
export async function createDynamicCommandProject(options?: {
  configFileName?: string;
  configContent?: string;
  manifestContent?: string;
  manifestFileName?: string;
  pluginContent?: string;
}) {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-dynamic-'));
  await writeProjectFile(
    cwd,
    options?.configFileName ?? 'lan.config.js',
    options?.configContent ??
      `export default {
        extensions: {
          dynamicCommands: true
        }
      };
      `,
  );
  if (options?.pluginContent) {
    await writeProjectFile(cwd, 'lania.plugin.js', options.pluginContent);
  }
  await writeProjectFile(
    cwd,
    options?.manifestFileName ?? 'lania.schemas.js',
    options?.manifestContent ??
      `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'ping',
                about: 'Ping',
                options: [{ long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true }],
                prompt: [{ field: 'endpoint', message: 'Endpoint?', kind: 'input', whenMissing: ['endpoint'] }],
                hooks: {
                  preRun: [async (ctx) => ({ events: [{ method: 'event.log', params: { level: 'info', message: 'preRun' } }] })]
                },
                handler: async (ctx) => ({ result: { ok: true, input: ctx.argv.options, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };
      `,
  );
  return cwd;
}

export async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}

export type TestHostRpcPayload = {
  id: string;
  method: string;
  params: Record<string, unknown>;
};

// Dynamic command tests repeatedly resolve a manifest command and immediately invoke
// it. Keep that flow here so split test files can stay focused on assertions.
export async function resolveManifestHandler(
  cwd: string,
  commandName: string,
  requestId = `req-${commandName}-resolve`,
) {
  const resolved = await handleExchange({
    id: requestId,
    method: 'commands.resolveDynamic',
    params: { cwd },
  });
  assert.equal(resolved.response.error, undefined);

  const handler = (resolved.response.result as any)?.handlers?.find(
    (entry: any) =>
      entry.target?.kind === 'manifest_command' && entry.target?.path?.join(' ') === commandName,
  );
  assert.ok(handler?.handlerId, `expected ${commandName} handlerId`);
  return handler as { handlerId: string; target: Record<string, unknown> };
}

export async function invokeManifestHandler(options: {
  cwd: string;
  commandName: string;
  requestIdPrefix: string;
  argv?: {
    args: Record<string, unknown>;
    options: Record<string, unknown>;
  };
  traceId?: string;
}) {
  const handler = await resolveManifestHandler(
    options.cwd,
    options.commandName,
    `${options.requestIdPrefix}-resolve`,
  );
  const invoked = await handleExchange({
    id: `${options.requestIdPrefix}-invoke`,
    method: 'command.invokeDynamic',
    params: {
      cwd: options.cwd,
      handlerId: handler.handlerId,
      argv: options.argv ?? { args: {}, options: {} },
      target: handler.target,
      ...(options.traceId ? { traceId: options.traceId } : {}),
    },
  });
  assert.equal(invoked.response.error, undefined);
  return invoked;
}

export function createHostRpcResponder(payload: Pick<TestHostRpcPayload, 'id'>) {
  return (result: unknown) => handleHostResponse({ id: payload.id, result, events: [] });
}

export function respondUnsupportedTestHostMethod(payload: Pick<TestHostRpcPayload, 'id' | 'method'>) {
  handleHostResponse({
    id: payload.id,
    error: {
      code: 'E_TEST_HOST_METHOD',
      message: `unsupported method ${payload.method}`,
    },
    events: [],
  });
}

export function invocationSafeString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export async function existsOnDisk(filePath: string): Promise<boolean> {
  try {
    await stat(filePath);
    return true;
  } catch {
    return false;
  }
}
