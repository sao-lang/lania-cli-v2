import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { handleExchange } from '../index.js';

test('hooks.invoke dispatches to project plugin through hooks.invoke', async (t) => {
  const cwd = await createLifecycleProject();
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const exchange = await handleExchange({
    id: 'req-2',
    method: 'hooks.invoke',
    params: {
      cwd,
      hook: 'onCommandPreInit',
      plugin: './scripts/lania.plugin.js',
      handler: 'validateEnv',
      source: 'host-runtime',
      kind: 'parallel',
      payload: {
        cwd,
        traceId: 't-1',
        command: { name: 'ops ping', handlerId: 'command.invokeDynamic' },
      },
    },
  });

  const result = exchange.response.result as
    | {
        accepted: boolean;
        hook: string;
        handler: string;
      }
    | undefined;

  assert.equal(exchange.response.error, undefined);
  assert.equal(result?.accepted, true);
  assert.equal(result?.hook, 'onCommandPreInit');
  assert.equal(result?.handler, 'validateEnv');
  assert.equal(exchange.events[0]?.method, 'event.log');
  const params =
    typeof exchange.events[0]?.params === 'object' && exchange.events[0]?.params !== null
      ? (exchange.events[0]?.params as Record<string, unknown>)
      : {};
  assert.match(String(params.message ?? ''), /validateEnv/);
});

async function createLifecycleProject() {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-lifecycle-'));
  await writeProjectFile(
    cwd,
    'lan.config.js',
    `export default {
      plugins: [
        './scripts/lania.plugin.js'
      ]
    };
    `,
  );
  await writeProjectFile(
    cwd,
    'scripts/lania.plugin.js',
    `export default {
      name: 'demo-lifecycle',
      methods: ['hooks.invoke'],
      async handle(method, params) {
        if (method !== 'hooks.invoke') {
          return null;
        }
        return {
          result: {
            accepted: true,
            hook: params.hook,
            handler: params.handler
          },
          events: [
            {
              method: 'event.log',
              params: {
                level: 'info',
                message: 'hook invoked: ' + params.handler
              }
            }
          ]
        };
      }
    };
    `,
  );
  return cwd;
}

async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}
