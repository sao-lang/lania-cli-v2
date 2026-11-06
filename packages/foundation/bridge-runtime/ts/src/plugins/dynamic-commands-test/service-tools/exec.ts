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

export function registerExecServiceTests() {
  test('command.invokeDynamic supports exec builder and service helpers', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        if (payload.method === 'host.exec.run' || payload.method === 'host.exec.runChecked') {
          respond({
            exitCode: 0,
            stdout: JSON.stringify(payload.params),
            stderr: '',
            skipped: false,
            timedOut: false,
            cancelled: false,
          });
          return;
        }

        respondUnsupportedTestHostMethod(payload);
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
                name: 'exec-builder',
                handler: async (ctx) => {
                  const builder = ctx.tools.exec
                    .command('node')
                    .withArgs(['script.js'])
                    .inDir('packages/demo')
                    .withEnv('NODE_ENV', 'test');
                  const builderRun = await builder.run({ timeoutMs: 123 });
                  const builderChecked = await ctx.tools.exec
                    .shell('echo hi')
                    .inDir('scripts')
                    .runChecked({ timeoutMs: 456 });
                  const directRun = await ctx.tools.exec.runWithOptions(
                    { program: 'pnpm', args: ['lint'], cwd: 'apps/web' },
                    { env: { CI: '1' } }
                  );
                  const directChecked = await ctx.tools.exec.runChecked({
                    program: 'pnpm',
                    args: ['test'],
                    cwd: 'apps/api'
                  });
                  const spawned = await ctx.tools.exec.spawn('git', ['status'], { cwd: 'repo' });
                  const spawnedChecked = await ctx.tools.exec.spawnChecked('git', ['diff'], { cwd: 'repo' });
                  return ctx.tools.result.ok({
                    workingDir: ctx.tools.exec.workingDir(),
                    builderRun: JSON.parse(builderRun.stdout),
                    builderChecked: JSON.parse(builderChecked.stdout),
                    directRun: JSON.parse(directRun.stdout),
                    directChecked: JSON.parse(directChecked.stdout),
                    spawned: JSON.parse(spawned.stdout),
                    spawnedChecked: JSON.parse(spawnedChecked.stdout)
                  });
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
      commandName: 'exec-builder',
      requestIdPrefix: 'req-exec-builder',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.workingDir, cwd);
    assert.equal(payload?.builderRun?.program, 'node');
    assert.deepEqual(payload?.builderRun?.args, ['script.js']);
    assert.equal(payload?.builderRun?.cwd, 'packages/demo');
    assert.equal(payload?.builderRun?.env?.NODE_ENV, 'test');
    assert.equal(payload?.builderRun?.timeoutMs, 123);
    assert.equal(payload?.builderRun?.useShell, false);
    assert.equal(payload?.builderChecked?.program, 'echo hi');
    assert.equal(payload?.builderChecked?.cwd, 'scripts');
    assert.equal(payload?.builderChecked?.useShell, true);
    assert.equal(payload?.directRun?.program, 'pnpm');
    assert.deepEqual(payload?.directRun?.args, ['lint']);
    assert.equal(payload?.directRun?.cwd, 'apps/web');
    assert.equal(payload?.directRun?.env?.CI, '1');
    assert.equal(payload?.directChecked?.program, 'pnpm');
    assert.deepEqual(payload?.directChecked?.args, ['test']);
    assert.equal(payload?.directChecked?.cwd, 'apps/api');
    assert.equal(payload?.spawned?.program, 'git');
    assert.deepEqual(payload?.spawned?.args, ['status']);
    assert.equal(payload?.spawnedChecked?.program, 'git');
    assert.deepEqual(payload?.spawnedChecked?.args, ['diff']);
  });
}
