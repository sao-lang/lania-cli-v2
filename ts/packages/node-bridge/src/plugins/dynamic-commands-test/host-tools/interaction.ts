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

export function registerInteractionToolTests() {
  test('command.invokeDynamic can use ctx.tools.interaction helpers', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        if (payload.method === 'host.interaction.input') {
          respond({ answer: 'demo-input' });
          return;
        }
        if (payload.method === 'host.interaction.confirm') {
          respond({ answer: true });
          return;
        }
        if (payload.method === 'host.interaction.select') {
          respond({ answer: 'b' });
          return;
        }
        if (payload.method === 'host.interaction.multiSelect') {
          respond({ answer: ['a', 'c'] });
          return;
        }
        if (payload.method === 'host.interaction.prompt') {
          respond({
            current_step_id: null,
            answers: { name: 'demo', env: 'dev' },
            context: {},
            completed_steps: [],
            timed_out_steps: [],
            interrupted: false,
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
                name: 'interact',
                handler: async (ctx) => {
                  const name = await ctx.tools.interaction.input({ field: 'name', message: 'Name?' });
                  const ok = await ctx.tools.interaction.confirm({ field: 'ok', message: 'Ok?' });
                  const picked = await ctx.tools.interaction.select({
                    field: 'pick',
                    message: 'Pick?',
                    choices: [{ label: 'A', value: 'a' }, { label: 'B', value: 'b' }],
                  });
                  const picks = await ctx.tools.interaction.multiSelect({
                    field: 'picks',
                    message: 'Picks?',
                    choices: [{ label: 'A', value: 'a' }, { label: 'B', value: 'b' }, { label: 'C', value: 'c' }],
                  });
                  const state = await ctx.tools.interaction.prompt([
                    { field: 'name', message: 'Name?', defaultValue: 'x' },
                    { field: 'env', message: 'Env?', defaultValue: 'dev' },
                  ]);
                  return ctx.tools.result.ok({ name, ok, picked, picks, promptAnswers: state.answers });
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
      commandName: 'interact',
      requestIdPrefix: 'req-interaction',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.name, 'demo-input');
    assert.equal(payload?.ok, true);
    assert.equal(payload?.picked, 'b');
    assert.deepEqual(payload?.picks, ['a', 'c']);
    assert.equal(payload?.promptAnswers?.name, 'demo');
    assert.equal(payload?.promptAnswers?.env, 'dev');
  });
}
