import test from 'node:test';

import {
  assert,
  createDynamicCommandProject,
  createHostRpcResponder,
  installHostRpcTransport,
  invokeManifestHandler,
  resetHostRpcTransport,
  rm,
  respondUnsupportedTestHostMethod,
  type TestHostRpcPayload,
  writeProjectFile,
} from '../shared.js';

export function registerToolsPolicyTests() {
  test('command.invokeDynamic enforces tools policy from lan.config tools allow/deny', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        if (payload.method === 'host.pm.detect') {
          respond({ manager: 'pnpm' });
          return;
        }
        if (payload.method === 'host.exec.run') {
          respond({ exitCode: 0, stdout: '', stderr: '' });
          return;
        }
        if (payload.method === 'host.fs.write') {
          respond({ ok: true });
          return;
        }

        respondUnsupportedTestHostMethod(payload);
      },
    });
    t.after(() => resetHostRpcTransport());

    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        tools: {
          allow: ['pm', 'exec', 'fs'],
          deny: ['exec'],
          exec: { allowShell: false, allowEnvWrite: false },
          fs: { writeRoot: '.allowed' },
          bridge: { allowMethods: ['config.*'] }
        }
      };`,
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'policy',
                handler: async (ctx) => {
                  const pm = await ctx.tools.pm.detect();
                  let execDenied = false;
                  let fsDenied = false;
                  let bridgeDenied = false;
                  try {
                    await ctx.tools.exec.run({ program: 'echo', args: ['x'], useShell: false });
                  } catch (error) {
                    execDenied = String(error).includes('E_TOOLS_DENIED');
                  }
                  try {
                    await ctx.tools.fs.write('../outside.txt', 'x');
                  } catch (error) {
                    fsDenied = String(error).includes('E_TOOLS_DENIED');
                  }
                  try {
                    await ctx.tools.bridge.call('bridge.shutdown');
                  } catch (error) {
                    bridgeDenied = String(error).includes('E_TOOLS_DENIED');
                  }
                  return ctx.tools.result.ok({ pm, execDenied, fsDenied, bridgeDenied });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(cwd, '.allowed/.gitkeep', '');

    const invoked = await invokeManifestHandler({
      cwd,
      commandName: 'policy',
      requestIdPrefix: 'req-tools-policy',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.pm, 'pnpm');
    assert.equal(payload?.execDenied, true);
    assert.equal(payload?.fsDenied, true);
    assert.equal(payload?.bridgeDenied, true);
  });
}
