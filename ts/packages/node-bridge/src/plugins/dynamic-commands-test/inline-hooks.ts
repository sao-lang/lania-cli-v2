// 内联 hook 用例，覆盖执行、ctx.tools 注入与审计事件。
import test from 'node:test';

import {
  assert,
  createDynamicCommandProject,
  handleExchange,
  rm,
  handleHostResponse,
  installHostRpcTransport,
  resetHostRpcTransport,
  invocationSafeString,
} from './shared.js';

export function registerTests() {
  test('hooks.invokeInline executes inline hook functions from manifest hooks', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default { extensions: { dynamicCommands: true } };`,
      pluginContent: `export default { name: 'noop', methods: [] };`,
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'ping',
                hooks: {
                  onArgsParsed: [
                    (payload) => ({
                      ...payload,
                      argv: {
                        ...payload.argv,
                        options: { ...(payload.argv?.options ?? {}), foo: 'bar' }
                      }
                    })
                  ]
                },
                handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-inline-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    assert.equal(resolved.response.error, undefined);
    const result = resolved.response.result as any;
    const handler = result.handlers.find((h: any) => h.target?.kind === 'manifest_command');
    const inlineId = handler?.target?.hooks?.onArgsParsed?.find(
      (b: any) => b.type === 'inline',
    )?.id;
    assert.equal(typeof inlineId, 'string');

    const invoked = await handleExchange({
      id: 'req-inline-2',
      method: 'hooks.invokeInline',
      params: {
        cwd,
        source: 'test',
        hook: 'onArgsParsed',
        kind: 'waterfall',
        id: inlineId,
        payload: { argv: { args: {}, options: {} } },
      },
    });
    assert.equal(invoked.response.error, undefined);
    const invokedPayload = (invoked.response.result as any)?.payload;
    assert.equal(invokedPayload?.argv?.options?.foo, 'bar');
  });

  test('hooks.invokeInline injects ctx.tools for inline hook functions', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'ping',
                hooks: {
                  onArgsParsed: [
                    (payload, ctx) => ({
                      ...payload,
                      argv: {
                        ...payload.argv,
                        options: {
                          ...(payload.argv?.options ?? {}),
                          fileName: ctx.tools.path.basename('/tmp/demo.json'),
                          envCwd: ctx.tools.env.cwd(),
                          rendered: ctx.tools.text.render('ok', { prefix: '[', suffix: ']' })
                        }
                      }
                    })
                  ]
                },
                handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-inline-tools-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    assert.equal(resolved.response.error, undefined);
    const result = resolved.response.result as any;
    const handler = result.handlers.find((h: any) => h.target?.kind === 'manifest_command');
    const inlineId = handler?.target?.hooks?.onArgsParsed?.find(
      (b: any) => b.type === 'inline',
    )?.id;
    assert.equal(typeof inlineId, 'string');

    const invoked = await handleExchange({
      id: 'req-inline-tools-2',
      method: 'hooks.invokeInline',
      params: {
        cwd,
        source: 'test',
        hook: 'onArgsParsed',
        kind: 'waterfall',
        id: inlineId,
        payload: { argv: { args: {}, options: {} } },
      },
    });

    const invokedPayload = (invoked.response.result as any)?.payload;
    assert.equal(invocationSafeString(invokedPayload?.argv?.options?.fileName), 'demo.json');
    assert.equal(invocationSafeString(invokedPayload?.argv?.options?.envCwd), cwd);
    assert.equal(invocationSafeString(invokedPayload?.argv?.options?.rendered), '[ok]');
  });

  test('hooks.invokeInline emits tool_call audit events for ctx.tools usage', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'ping',
                hooks: {
                  onArgsParsed: [
                    (payload, ctx) => ({
                      payload,
                      events: [
                        ctx.tools.result.event('event.log', {
                          level: 'info',
                          target: 'test',
                          message: ctx.tools.text.render('inline')
                        })
                      ]
                    })
                  ]
                },
                handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-inline-audit-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const result = resolved.response.result as any;
    const handler = result.handlers.find((h: any) => h.target?.kind === 'manifest_command');
    const inlineId = handler?.target?.hooks?.onArgsParsed?.find(
      (b: any) => b.type === 'inline',
    )?.id;
    assert.equal(typeof inlineId, 'string');

    const invoked = await handleExchange({
      id: 'req-inline-audit-2',
      method: 'hooks.invokeInline',
      params: {
        cwd,
        source: 'test',
        hook: 'onArgsParsed',
        kind: 'waterfall',
        id: inlineId,
        payload: { argv: { args: {}, options: {} } },
      },
    });

    const toolCallEvent: any = invoked.events.find(
      (event: any) => event.method === 'event.log' && event.params?.phase === 'tool_call',
    );
    assert.ok(toolCallEvent, 'expected tool_call event from inline hook');
    assert.equal(toolCallEvent?.params?.tool, 'text');
    assert.equal(toolCallEvent?.params?.methodName, 'render');
    assert.equal(toolCallEvent?.params?.ok, true);
  });
}
