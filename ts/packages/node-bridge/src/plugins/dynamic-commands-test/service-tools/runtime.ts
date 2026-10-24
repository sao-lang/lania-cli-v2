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
} from '../shared.js';

export function registerRuntimeServiceTests() {
  test('command.invokeDynamic supports runtime planning and execution helpers', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.runtime.list':
            respond({
              runtimes: [
                {
                  id: 'python',
                  displayName: 'Python',
                  extensions: ['py'],
                  candidates: [{ program: 'python3', args: [] }],
                  prepare: null,
                  packageManagers: ['pip', 'uv', 'poetry'],
                },
              ],
            });
            return;
          case 'host.runtime.detect':
            respond({
              file: String(payload.params.file),
              language: 'python',
              runtime: 'python3',
            });
            return;
          case 'host.runtime.spec':
            respond({
              id: String(payload.params.language),
              displayName: 'Python',
              extensions: ['py'],
              candidates: [],
              prepare: null,
              packageManagers: ['pip'],
            });
            return;
          case 'host.runtime.plan':
            respond({
              runtime: 'python3',
              language: 'python',
              program: 'python3',
              args: ['demo.py'],
              cwd: payload.params.cwd,
              preparedCommand: null,
              preparedSummary: null,
            });
            return;
          case 'host.runtime.run':
            respond({
              file: String(payload.params.file),
              runtime: 'python3',
              language: 'python',
              program: 'python3',
              args: ['demo.py'],
              cwd: payload.params.cwd,
              preparedCommand: null,
              preparedSummary: null,
              exitCode: 0,
              stdout: 'runtime-ok',
              stderr: '',
              skipped: false,
              timedOut: false,
              cancelled: false,
            });
            return;
          default:
            respondUnsupportedTestHostMethod(payload);
        }
      },
    });
    t.after(() => resetHostRpcTransport());

    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'runtime-facade',
                handler: async (ctx) => {
                  const list = await ctx.tools.runtime.list();
                  const detected = await ctx.tools.runtime.detect('demo.py');
                  const spec = await ctx.tools.runtime.spec('python');
                  const plan = await ctx.tools.runtime.plan('demo.py');
                  const run = await ctx.tools.runtime.run('demo.py');
                  return ctx.tools.result.ok({ list, detected, spec, plan, run });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const invoked = await invokeManifestHandler({
      cwd,
      commandName: 'runtime-facade',
      requestIdPrefix: 'req-runtime-facade',
    });

    // `invokeManifestHandler` returns a generic bridge exchange; this test owns the mock shape.
    type InvokeData = {
      result?: {
        data?: {
          list?: Array<{ id?: string }>;
          detected?: { language?: string };
          spec?: { id?: string };
          plan?: { program?: string };
          run?: { exitCode?: number; stdout?: string };
        };
      };
    };
    const payload = (invoked.response.result as InvokeData | undefined)?.result?.data;
    assert.equal(payload?.list?.[0]?.id, 'python');
    assert.equal(payload?.detected?.language, 'python');
    assert.equal(payload?.spec?.id, 'python');
    assert.equal(payload?.plan?.program, 'python3');
    assert.equal(payload?.run?.exitCode, 0);
    assert.equal(payload?.run?.stdout, 'runtime-ok');
  });
}
