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

export function registerTaskServiceTests() {
  test('command.invokeDynamic supports tasks executor helpers', async (t) => {
    const taskState = {
      tasks: [] as Array<Record<string, unknown>>,
      events: [] as Array<Record<string, unknown>>,
    };

    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.tasks.register':
            taskState.tasks.push({
              id: payload.params.id,
              title: payload.params.title,
              group: payload.params.group,
              priority: payload.params.priority,
              state: 'pending',
            });
            taskState.events.push({ kind: 'registered', taskId: payload.params.id });
            respond({ ok: true });
            return;
          case 'host.tasks.start': {
            const task = taskState.tasks.find((entry) => entry.id === payload.params.id);
            if (task) task.state = 'running';
            taskState.events.push({ kind: 'started', taskId: payload.params.id });
            respond({ ok: true });
            return;
          }
          case 'host.tasks.update':
            taskState.events.push({
              kind: 'updated',
              taskId: payload.params.id,
              detail: payload.params.detail,
            });
            respond({ ok: true });
            return;
          case 'host.tasks.complete': {
            const task = taskState.tasks.find((entry) => entry.id === payload.params.id);
            if (task) task.state = 'completed';
            taskState.events.push({ kind: 'completed', taskId: payload.params.id });
            respond({ ok: true });
            return;
          }
          case 'host.tasks.fail': {
            const task = taskState.tasks.find((entry) => entry.id === payload.params.id);
            if (task) task.state = 'failed';
            taskState.events.push({
              kind: 'failed',
              taskId: payload.params.id,
              detail: payload.params.detail,
            });
            respond({ ok: true });
            return;
          }
          case 'host.tasks.cancel': {
            const task = taskState.tasks.find((entry) => entry.id === payload.params.id);
            if (task) task.state = 'cancelled';
            taskState.events.push({ kind: 'cancelled', taskId: payload.params.id });
            respond({ ok: true });
            return;
          }
          case 'host.tasks.snapshot':
            respond({ tasks: taskState.tasks });
            return;
          case 'host.tasks.events':
            respond({ events: taskState.events });
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
                name: 'task-executor',
                handler: async (ctx) => {
                  const executor = ctx.tools.tasks.create({ stopOnError: false });
                  executor.pause();
                  const pausedBefore = executor.isPaused();
                  executor.resume();
                  executor.add({
                    id: 'one',
                    title: 'one',
                    detail: 'first',
                    run: async () => 'ok-1'
                  });
                  executor.addMany([
                    {
                      id: 'two',
                      title: 'two',
                      group: 'blocked',
                      run: async () => 'should-not-run'
                    },
                    {
                      id: 'three',
                      title: 'three',
                      run: async () => 'ok-3'
                    }
                  ]);
                  const queuedBefore = executor.queue().map((task) => task.id);
                  executor.cancelGroup('blocked');
                  const runResults = await executor.run();
                  const topLevelResults = await ctx.tools.tasks.run([
                    {
                      id: 'four',
                      title: 'four',
                      run: async () => 'ok-4'
                    }
                  ]);
                  const snapshot = await ctx.tools.tasks.snapshot();
                  const events = await ctx.tools.tasks.events();
                  return ctx.tools.result.ok({
                    pausedBefore,
                    queuedBefore,
                    runResults,
                    topLevelResults,
                    runningAfter: executor.running(),
                    resultsAfter: executor.results(),
                    isRunningAfter: executor.isRunning(),
                    snapshotCount: snapshot.tasks.length,
                    eventKinds: events.events.map((event) => event.kind)
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
      commandName: 'task-executor',
      requestIdPrefix: 'req-task-executor',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.pausedBefore, true);
    assert.deepEqual(payload?.queuedBefore, ['one', 'two', 'three']);
    assert.deepEqual(
      payload?.runResults?.map((item: any) => [item.id, item.status]),
      [
        ['one', 'completed'],
        ['two', 'cancelled'],
        ['three', 'completed'],
      ],
    );
    assert.deepEqual(
      payload?.topLevelResults?.map((item: any) => [item.id, item.status]),
      [['four', 'completed']],
    );
    assert.deepEqual(payload?.runningAfter, []);
    assert.equal(payload?.isRunningAfter, false);
    assert.equal(payload?.snapshotCount, 4);
    assert.ok(payload?.eventKinds?.includes('registered'));
    assert.ok(payload?.eventKinds?.includes('completed'));
    assert.ok(payload?.eventKinds?.includes('cancelled'));
  });
}
