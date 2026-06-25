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

export function registerHostFacadeTests() {
  test('command.invokeDynamic can call host-backed tools via ctx.tools', async (t) => {
    const taskState = {
      tasks: [] as Array<Record<string, unknown>>,
      events: [] as Array<Record<string, unknown>>,
    };
    const logState = {
      entries: [] as Array<Record<string, unknown>>,
    };
    const progressState = {
      items: [] as Array<Record<string, unknown>>,
      events: [] as Array<Record<string, unknown>>,
      nextSequence: 0,
      terminalSuspended: false,
    };

    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.fs.write':
            respond({ ok: true });
            return;
          case 'host.fs.exists':
            respond({ exists: true });
            return;
          case 'host.fs.read':
            respond({ content: JSON.stringify({ ok: true, from: 'schema' }) });
            return;
          case 'host.log.emit':
            logState.entries.push({
              level: payload.params.level,
              target: payload.params.target,
              message: payload.params.message,
              phase: payload.params.phase,
              operation: payload.params.operation,
            });
            respond({ ok: true });
            return;
          case 'host.log.entries':
            respond(logState.entries);
            return;
          case 'host.log.clear':
            logState.entries.length = 0;
            respond({ ok: true });
            return;
          case 'host.tasks.register':
            taskState.tasks.push({ id: payload.params.id, title: payload.params.title });
            taskState.events.push({ kind: 'registered', taskId: payload.params.id });
            respond({ ok: true });
            return;
          case 'host.tasks.start':
          case 'host.tasks.update':
          case 'host.tasks.complete':
            taskState.events.push({
              kind: payload.method.split('.').pop(),
              taskId: payload.params.id,
            });
            respond({ ok: true });
            return;
          case 'host.tasks.snapshot':
            respond({ tasks: taskState.tasks });
            return;
          case 'host.progress.begin':
          case 'host.progress.beginGroup':
            progressState.items.push({
              id: payload.params.id,
              current: 0,
              total: payload.params.total ?? null,
              kind: payload.params.kind ?? 'progress_bar',
              status: 'running',
            });
            progressState.events.push({
              sequence: ++progressState.nextSequence,
              progressId: payload.params.id,
              kind: 'began',
            });
            respond({ ok: true });
            return;
          case 'host.progress.beginStep':
            progressState.items.push({
              id: payload.params.id,
              parentId: payload.params.parentId,
              current: 0,
              total: payload.params.total ?? null,
              kind: payload.params.kind ?? 'static_step',
              status: 'running',
            });
            progressState.events.push({
              sequence: ++progressState.nextSequence,
              progressId: payload.params.id,
              kind: 'began',
            });
            respond({ ok: true });
            return;
          case 'host.progress.advance': {
            const item = progressState.items.find((entry) => entry.id === payload.params.id);
            if (item) {
              item.current = Number(item.current ?? 0) + Number(payload.params.delta ?? 1);
            }
            progressState.events.push({
              sequence: ++progressState.nextSequence,
              progressId: payload.params.id,
              kind: 'advanced',
            });
            respond({ ok: true });
            return;
          }
          case 'host.progress.finish': {
            const item = progressState.items.find((entry) => entry.id === payload.params.id);
            if (item) {
              item.status = 'completed';
            }
            progressState.events.push({
              sequence: ++progressState.nextSequence,
              progressId: payload.params.id,
              kind: 'finished',
            });
            respond({ ok: true });
            return;
          }
          case 'host.progress.summary':
            respond({ items: progressState.items, events: progressState.events });
            return;
          case 'host.progress.contains':
            respond({
              contains: progressState.items.some((entry) => entry.id === payload.params.id),
            });
            return;
          case 'host.progress.render':
            respond({ lines: progressState.items.map((entry) => `${entry.id}:${entry.status}`) });
            return;
          case 'host.progress.suspendTerminal':
            progressState.terminalSuspended = true;
            respond({ ok: true });
            return;
          case 'host.progress.resumeTerminal':
            progressState.terminalSuspended = false;
            respond({ ok: true });
            return;
          case 'host.exec.run':
            respond({
              exitCode: 0,
              stdout:
                `${payload.params.program} ${(payload.params.args as string[] | undefined)?.join(' ') ?? ''}`.trim(),
              stderr: '',
              skipped: false,
              timedOut: false,
              cancelled: false,
            });
            return;
          case 'host.pm.command.runScript':
            respond({ program: 'pnpm', args: ['run', String(payload.params.script)] });
            return;
          case 'host.git.changedFiles':
          case 'host.git.workspace.changedFiles':
            respond({ files: ['src/index.ts', 'README.md'] });
            return;
          case 'host.pm.detect':
            respond({ manager: 'pnpm' });
            return;
          case 'host.interaction.input':
          case 'host.interaction.confirm':
          case 'host.interaction.select':
          case 'host.interaction.multiSelect':
          case 'host.interaction.password':
          case 'host.interaction.editor':
            // Return the provided defaultValue as the answer to keep this test non-interactive.
            respond({ answer: (payload.params as any).defaultValue ?? null });
            return;
          case 'host.interaction.prompt':
          case 'host.interaction.flow.execute': {
            const questions = Array.isArray((payload.params as any).questions)
              ? (payload.params as any).questions
              : [];
            const answers: Record<string, unknown> = {};
            for (const question of questions) {
              const field = typeof question?.field === 'string' ? question.field : 'value';
              answers[field] = question?.defaultValue ?? null;
            }
            respond({
              current_step_id: null,
              answers,
              context: {},
              completed_steps: [],
              timed_out_steps: [],
              interrupted: false,
            });
            return;
          }
          case 'host.interaction.resetAccumulated':
            respond({ ok: true });
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
                name: 'hosted',
                handler: async (ctx) => {
                  const tmp = ctx.tools.path.resolve(ctx.cwd, '.lania-tools-host-test.json');
                  await ctx.tools.fs.writeJson(tmp, { ok: true, from: 'schema' }, { space: 2 });
                  const exists = await ctx.tools.fs.exists(tmp);
                  const data = await ctx.tools.fs.readJson(tmp);
                  const execResult = await ctx.tools.exec.run({ program: 'echo', args: ['hello'] });
                  const runScript = await ctx.tools.pm.command.runScript('build');
                  const changedFiles = await ctx.tools.git.changedFiles();

                  await ctx.tools.log.log('schema log default');
                  await ctx.tools.log.info('schema log from handler', { target: 'schema.test' });
                  await ctx.tools.log.success('schema success');
                  await ctx.tools.log.scoped('worker').warn('scoped warn');
                  const logEntries = await ctx.tools.log.entries();

                  await ctx.tools.tasks.register('t1', 'task one', { group: 'g', priority: 'low' });
                  await ctx.tools.tasks.start('t1', 'task one');
                  await ctx.tools.tasks.update('t1', 'running');

                  await ctx.tools.progress.beginGroup('p1', 1, 'progress_bar');
                  await ctx.tools.progress.beginStep('p1.step', 'p1', 1, 'static_step');
                  await ctx.tools.progress.advance('p1.step', 1);
                  await ctx.tools.progress.finish('p1.step');
                  const containsStep = await ctx.tools.progress.contains('p1.step');
                  const renderLines = await ctx.tools.progress.render();
                  await ctx.tools.progress.suspendTerminal();
                  await ctx.tools.progress.resumeTerminal();
                  const callbackEvents = [];
                  const unsubscribe = ctx.tools.progress.onProgress((snapshot, event) => {
                    callbackEvents.push({ snapshot, event });
                  });
                  await ctx.tools.progress.begin('p2', 2);
                  await ctx.tools.progress.advance('p2', 1);
                  await ctx.tools.progress.finish('p2');
                  unsubscribe();

                  const snap = await ctx.tools.tasks.snapshot();
                  const pm = await ctx.tools.pm.detect();
                  return ctx.tools.result.ok({
                    exists,
                    data,
                    tasks: snap.tasks.length,
                    pm,
                    execStdout: execResult.stdout,
                    runScriptProgram: runScript.program,
                    changedFiles,
                    logEntries,
                    containsStep,
                    renderLines,
                    callbackEvents
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
      commandName: 'hosted',
      requestIdPrefix: 'req-host-tools',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.exists, true);
    assert.equal(payload?.data?.ok, true);
    assert.equal(typeof payload?.pm, 'string');
    assert.equal(payload?.tasks, 1);
    assert.equal(payload?.execStdout, 'echo hello');
    assert.equal(payload?.runScriptProgram, 'pnpm');
    assert.deepEqual(payload?.changedFiles, ['src/index.ts', 'README.md']);
    assert.equal(Array.isArray(payload?.logEntries), true);
    assert.equal(payload?.logEntries?.length, 4);
    assert.deepEqual(
      payload?.logEntries?.map((entry: any) => [
        entry.level,
        entry.target,
        entry.phase,
        entry.operation,
      ]),
      [
        ['info', 'schema', 'event.log', 'event.log'],
        ['info', 'schema.test', 'event.log', 'event.log'],
        ['info', 'schema', 'event.log', 'event.log'],
        ['warn', 'schema.worker', 'event.log', 'event.log'],
      ],
    );
    assert.equal(payload?.containsStep, true);
    assert.deepEqual(payload?.renderLines, ['p1:running', 'p1.step:completed']);
    assert.equal(Array.isArray(payload?.callbackEvents), true);
    assert.equal(payload?.callbackEvents?.length, 3);
  });
}
