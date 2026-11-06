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

export function registerToolAuditTests() {
  test('command.invokeDynamic emits tool_call audit events for ctx.tools usage', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        if (payload.method === 'host.pm.detect') {
          respond({ manager: 'pnpm' });
          return;
        }
        if (payload.method === 'host.git.branch.current') {
          respond({ branch: 'main' });
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
                name: 'audit',
                handler: async (ctx) => {
                  const pm = await ctx.tools.pm.detect();
                  const branch = await ctx.tools.git.branch.current();
                  const ping = await ctx.tools.bridge.call('bridge.ping');
                  return ctx.tools.result.ok({ pm, branch, ping: ping.response.result });
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
      commandName: 'audit',
      requestIdPrefix: 'req-tools-audit',
      traceId: 'trace-tools-audit',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.pm, 'pnpm');
    assert.equal(payload?.branch, 'main');
    assert.equal(payload?.ping?.ok, true);

    const toolCallEvents = invoked.events.filter(
      (event: any) => event.method === 'event.log' && event.params?.phase === 'tool_call',
    );
    const methods = toolCallEvents.map(
      (event: any) => `${event.params?.tool}.${event.params?.methodName}`,
    );
    assert.ok(toolCallEvents.length >= 3, 'expected tool_call event');
    assert.equal(methods.includes('pm.detect'), true);
    assert.equal(methods.includes('git.branch.current'), true);
    assert.equal(methods.includes('bridge.call'), true);
    assert.equal(
      toolCallEvents.every((event: any) => event.params?.ok === true),
      true,
    );
    assert.equal(
      toolCallEvents.every((event: any) => event.params?.traceId === 'trace-tools-audit'),
      true,
    );
  });
}
