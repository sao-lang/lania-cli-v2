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

export function registerPackageManagerServiceTests() {
  test('command.invokeDynamic supports package manager planning and execution helpers', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.pm.detect':
            respond({ manager: 'pnpm' });
            return;
          case 'host.pm.supportedManagers':
            respond({ managers: ['npm', 'pnpm', 'yarn', 'bun'] });
            return;
          case 'host.pm.spec':
            respond({
              manager: 'pnpm',
              binary: 'pnpm',
              install_subcommand: 'install',
              run_subcommand: 'run',
              lockfile: 'pnpm-lock.yaml',
            });
            return;
          case 'host.pm.loadPackageJsonSnapshot':
            respond({
              exists: true,
              path: `${payload.params.cwd}/package.json`,
              scripts: { dev: 'vite', build: 'vite build' },
              raw: {},
            });
            return;
          case 'host.pm.scriptExists':
            respond({
              exists: payload.params.script === 'dev' || payload.params.script === 'build',
            });
            return;
          case 'host.pm.command.install':
            respond({
              program: 'pnpm',
              args: [
                'install',
                '--strict-peer-dependencies=false',
                ...(payload.params.packages as string[]),
              ],
            });
            return;
          case 'host.pm.command.runScript':
            respond({
              program: 'pnpm',
              args: ['run', String(payload.params.script), ...(payload.params.args as string[])],
            });
            return;
          case 'host.exec.run':
          case 'host.exec.runChecked':
            respond({
              exitCode: 0,
              stdout: JSON.stringify(payload.params),
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
                name: 'pm-facade',
                handler: async (ctx) => {
                  const detectedFromFiles = ctx.tools.pm.detectFromFiles(['package.json', 'yarn.lock']);
                  const binary = ctx.tools.pm.binary('bun');
                  const lockfile = ctx.tools.pm.lockfile('pnpm');
                  const lockfileStrategy = ctx.tools.pm.lockfileStrategy('yarn');
                  const spec = await ctx.tools.pm.spec('npm');
                  const initCommand = await ctx.tools.pm.command.init('npm');
                  const installAllCommand = await ctx.tools.pm.command.installAll('pnpm');
                  const removeCommand = await ctx.tools.pm.command.remove(['eslint'], { manager: 'yarn' });
                  const updateCommand = await ctx.tools.pm.command.update(['react'], { manager: 'bun' });
                  const publishCommand = await ctx.tools.pm.command.publish({ manager: 'pnpm', tag: 'next' });
                  const addDependencyCommands = await ctx.tools.pm.command.addDependencyCommands({
                    manager: 'pnpm',
                    dependencies: ['react'],
                    devDependencies: ['typescript']
                  });
                  await ctx.tools.pm.requireScript('dev');
                  const runScriptChecked = await ctx.tools.pm.command.runScriptChecked('build', { manager: 'pnpm', args: ['--watch'] });
                  const installResult = await ctx.tools.pm.install(['react'], { manager: 'pnpm', dev: true });
                  const installAllResult = await ctx.tools.pm.installAll({ manager: 'npm' });
                  const runResult = await ctx.tools.pm.run('dev', ['--host'], { manager: 'pnpm', checked: true });
                  const publishResult = await ctx.tools.pm.publish({ manager: 'pnpm', tag: 'beta' });
                  return ctx.tools.result.ok({
                    detectedFromFiles,
                    binary,
                    lockfile,
                    lockfileStrategy,
                    spec,
                    initCommand,
                    installAllCommand,
                    removeCommand,
                    updateCommand,
                    publishCommand,
                    addDependencyCommands,
                    runScriptChecked,
                    installResult: JSON.parse(installResult.stdout),
                    installAllResult: JSON.parse(installAllResult.stdout),
                    runResult: JSON.parse(runResult.stdout),
                    publishResult: JSON.parse(publishResult.stdout)
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
      commandName: 'pm-facade',
      requestIdPrefix: 'req-pm-facade',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.detectedFromFiles, 'yarn');
    assert.equal(payload?.binary, 'bun');
    assert.equal(payload?.lockfile, 'pnpm-lock.yaml');
    assert.equal(payload?.lockfileStrategy, 'yarn uses yarn.lock');
    assert.equal(payload?.spec?.binary, 'npm');
    assert.deepEqual(payload?.initCommand, { program: 'npm', args: ['init', '-y'] });
    assert.deepEqual(payload?.installAllCommand, {
      program: 'pnpm',
      args: ['install', '--strict-peer-dependencies=false'],
    });
    assert.deepEqual(payload?.removeCommand, { program: 'yarn', args: ['remove', 'eslint'] });
    assert.deepEqual(payload?.updateCommand, { program: 'bun', args: ['update', 'react'] });
    assert.deepEqual(payload?.publishCommand, {
      program: 'pnpm',
      args: ['publish', '--tag', 'next'],
    });
    assert.deepEqual(payload?.addDependencyCommands, [
      { program: 'pnpm', args: ['install', '--strict-peer-dependencies=false', 'react'] },
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', '--save-dev', 'typescript'],
      },
    ]);
    assert.deepEqual(payload?.runScriptChecked, {
      program: 'pnpm',
      args: ['run', 'build', '--', '--watch'],
    });
    assert.equal(payload?.installResult?.program, 'pnpm');
    assert.deepEqual(payload?.installResult?.args, [
      'install',
      '--strict-peer-dependencies=false',
      '--save-dev',
      'react',
    ]);
    assert.equal(payload?.installAllResult?.program, 'npm');
    assert.deepEqual(payload?.installAllResult?.args, ['install', '--legacy-peer-deps']);
    assert.equal(payload?.runResult?.program, 'pnpm');
    assert.deepEqual(payload?.runResult?.args, ['run', 'dev', '--', '--host']);
    assert.equal(payload?.publishResult?.program, 'pnpm');
    assert.deepEqual(payload?.publishResult?.args, ['publish', '--tag', 'beta']);
  });
}
