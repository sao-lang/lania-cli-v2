// 桥接 facade 用例，覆盖原始 bridge surface 与编译/ lint 别名。
import test from 'node:test';

import { assert, createDynamicCommandProject, handleExchange, rm } from './shared.js';

export function registerTests() {
  test('command.invokeDynamic supports bridge raw surface and request builders', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'bridge-raw',
                handler: async (ctx) => {
                  const observed = [];
                  const unsubscribe = ctx.tools.bridge.subscribeEvents((event) => {
                    observed.push(event.method);
                  });
                  const handshake = ctx.tools.bridge.handshakeRequest();
                  const pingRequest = ctx.tools.bridge.pingRequest();
                  const metricsRequest = ctx.tools.bridge.metricsRequest();
                  const subscribeRequest = ctx.tools.bridge.subscribeRequest();
                  const loadLanRequest = ctx.tools.bridge.loadLanConfigRequest();
                  const loadToolRequest = ctx.tools.bridge.loadToolConfigRequest(ctx.cwd, 'commitlint');
                  const compilerDevRequest = ctx.tools.bridge.compilerDevRequest(ctx.cwd, 3001);
                  const compilerBuildRequest = ctx.tools.bridge.compilerBuildRequest(ctx.cwd, true);
                  const compilerBuildWithOptionsRequest = ctx.tools.bridge.compilerBuildWithOptionsRequest(
                    ctx.cwd,
                    false,
                    'production',
                    'dist'
                  );
                  const compilerStopRequest = ctx.tools.bridge.compilerStopRequest();
                  const lintRunRequest = ctx.tools.bridge.lintRunRequest(ctx.cwd, true, 2);
                  const commitizenRunRequest = ctx.tools.bridge.commitizenRunRequest(
                    ctx.cwd,
                    'feat',
                    'core',
                    'add bridge'
                  );
                  const commitlintRunRequest = ctx.tools.bridge.commitlintRunRequest(
                    ctx.cwd,
                    'feat(core): short'
                  );
                  const ping = await ctx.tools.bridge.call(pingRequest);
                  const metrics = await ctx.tools.bridge.call(metricsRequest);
                  const subscribe = await ctx.tools.bridge.call(subscribeRequest);
                  const lint = await ctx.tools.bridge.callAsync(commitlintRunRequest);
                  const shutdown = await ctx.tools.bridge.openCall('bridge.shutdown');
                  const shutdownAsync = await ctx.tools.bridge.shutdownAsync();
                  const metricsSnapshot = ctx.tools.bridge.metricsSnapshot();
                  const usingProcessTransport = ctx.tools.bridge.usingProcessTransport();
                  const supportedEvents = ctx.tools.bridge.supportedEvents();
                  const timeout = ctx.tools.bridge.timeout();
                  unsubscribe();
                  return ctx.tools.result.ok({
                    handshake,
                    pingRequest,
                    metricsRequest,
                    subscribeRequest,
                    loadLanRequest,
                    loadToolRequest,
                    compilerDevRequest,
                    compilerBuildRequest,
                    compilerBuildWithOptionsRequest,
                    compilerStopRequest,
                    lintRunRequest,
                    commitizenRunRequest,
                    commitlintRunRequest,
                    ping: ping.response.result,
                    metrics: metrics.response.result,
                    subscribe: subscribe.response.result,
                    lint: lint.response.result,
                    shutdown: shutdown.response.result,
                    shutdownEvents: shutdown.events,
                    shutdownAsync: shutdownAsync.response.result,
                    metricsSnapshot,
                    usingProcessTransport,
                    supportedEvents,
                    timeout,
                    observed
                  });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-bridge-raw-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    assert.equal(resolved.response.error, undefined);
    const resolveResult = resolved.response.result as any;
    const handler = resolveResult.handlers.find(
      (entry: any) =>
        entry.target.kind === 'manifest_command' && entry.target.path?.join(' ') === 'bridge-raw',
    );
    assert.ok(handler, 'expected bridge-raw handler');

    const invocation = await handleExchange({
      id: 'req-bridge-raw-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });
    assert.equal(invocation.response.error, undefined);
    const payload = (invocation.response.result as any)?.result?.data;
    assert.equal(payload?.handshake?.method, 'bridge.handshake');
    assert.equal(payload?.pingRequest?.method, 'bridge.ping');
    assert.equal(payload?.metricsRequest?.method, 'bridge.metrics');
    assert.equal(payload?.subscribeRequest?.method, 'bridge.subscribe');
    assert.equal(payload?.loadLanRequest?.method, 'config.loadLan');
    assert.equal(payload?.loadToolRequest?.params?.tool, 'commitlint');
    assert.equal(payload?.compilerDevRequest?.params?.port, 3001);
    assert.equal(payload?.compilerBuildRequest?.params?.watch, true);
    assert.equal(payload?.compilerBuildWithOptionsRequest?.params?.mode, 'production');
    assert.equal(payload?.compilerBuildWithOptionsRequest?.params?.outputDir, 'dist');
    assert.equal(payload?.compilerStopRequest?.method, 'compiler.stop');
    assert.equal(payload?.lintRunRequest?.params?.fix, true);
    assert.equal(payload?.lintRunRequest?.params?.concurrency, 2);
    assert.equal(payload?.commitizenRunRequest?.params?.kind, 'feat');
    assert.equal(payload?.commitizenRunRequest?.params?.scope, 'core');
    assert.equal(payload?.commitlintRunRequest?.params?.message, 'feat(core): short');
    assert.equal(payload?.ping?.ok, true);
    assert.equal(payload?.ping?.bridgeName, '@lania-cli/node-bridge');
    assert.equal(payload?.metrics?.requests >= 2, true);
    assert.equal(payload?.subscribe?.accepted, true);
    assert.equal(payload?.subscribe?.mode, 'request_response_stream');
    assert.equal(payload?.lint?.valid, true);
    assert.equal(payload?.shutdown?.accepted, true);
    assert.equal(payload?.shutdown?.stopped, true);
    assert.equal(payload?.shutdownAsync?.accepted, true);
    assert.equal(payload?.metricsSnapshot?.requests >= 4, true);
    assert.equal(payload?.usingProcessTransport, false);
    assert.equal(Array.isArray(payload?.supportedEvents), true);
    assert.equal(payload?.supportedEvents?.includes('event.shutdown'), true);
    assert.equal(payload?.timeout, 30000);
    assert.equal(Array.isArray(payload?.shutdownEvents), true);
    assert.equal(payload?.shutdownEvents?.[0]?.method, 'event.shutdown');
    assert.equal(Array.isArray(payload?.observed), true);
    assert.equal(payload?.observed?.includes('event.shutdown'), true);
  });

  test('command.invokeDynamic supports compiler and lint aliases', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'bridge-alias',
                handler: async (ctx) => {
                  const compilerDev = await ctx.tools.compiler.dev({ port: 3002 });
                  const compilerBuild = await ctx.tools.compiler.build({ watch: false, mode: 'production' });
                  const compilerStop = await ctx.tools.compiler.stop();
                  const lintRun = await ctx.tools.lint.run({ fix: true, concurrency: 2 });
                  const sameCompiler = ctx.tools.compiler === ctx.tools.bridge.compiler;
                  const sameLint = ctx.tools.lint === ctx.tools.bridge.lint;
                  return ctx.tools.result.ok({
                    compilerDev: compilerDev.response.result,
                    compilerBuild: compilerBuild.response.result,
                    compilerStop: compilerStop.response.result,
                    lintRun: lintRun.response.result,
                    sameCompiler,
                    sameLint
                  });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-bridge-alias-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    assert.equal(resolved.response.error, undefined);
    const resolveResult = resolved.response.result as any;
    const handler = resolveResult.handlers.find(
      (entry: any) =>
        entry.target.kind === 'manifest_command' && entry.target.path?.join(' ') === 'bridge-alias',
    );
    assert.ok(handler, 'expected bridge-alias handler');

    const invocation = await handleExchange({
      id: 'req-bridge-alias-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });
    assert.equal(invocation.response.error, undefined);
    const payload = (invocation.response.result as any)?.result?.data;
    assert.equal(payload?.compilerDev?.action, 'dev');
    assert.equal(payload?.compilerDev?.port, 3002);
    assert.equal(payload?.compilerBuild?.action, 'build');
    assert.equal(payload?.compilerBuild?.watch, false);
    assert.equal(payload?.compilerBuild?.mode, 'production');
    assert.equal(payload?.compilerStop?.stopped, true);
    assert.equal(payload?.lintRun?.fix, true);
    assert.equal(payload?.lintRun?.concurrency, 2);
    assert.equal(payload?.sameCompiler, true);
    assert.equal(payload?.sameLint, true);
  });
}
