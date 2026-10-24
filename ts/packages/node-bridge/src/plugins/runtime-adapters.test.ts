import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  getRuntimeAdapters,
  resolveRuntimeAdapter,
  resolveSpawnCommand,
} from './runtime-adapters/index.js';
import { compilerPlugin } from './compiler.js';
import type { CompilerAdapterContext, CompilerAdapterParams } from './compiler-shared.js';
import type { SpawnRuntime } from './runtime-adapters/types.js';

function params(
  cwd: string,
  overrides: Partial<CompilerAdapterParams> = {},
): CompilerAdapterParams {
  return {
    cwd,
    host: '127.0.0.1',
    port: 4321,
    hmr: null,
    open: false,
    watch: false,
    mode: null,
    outputDir: 'dist',
    ...overrides,
  };
}

function context(
  runtime: SpawnRuntime,
  lanConfig: Record<string, unknown>,
): CompilerAdapterContext {
  return { tool: 'vite', runtime, lanConfig, runtimeWarning: null };
}

const FIXTURE = `
const fs = require('node:fs');
const args = process.argv.slice(2);
if (process.env.FIXTURE_LOG) {
  fs.writeFileSync(process.env.FIXTURE_LOG, JSON.stringify({
    args,
    port: process.env.PORT ?? null,
    host: process.env.HOST ?? null,
    output: process.env.OUTPUT_DIR ?? null,
  }));
}
process.stdout.write('started\\n');
process.stderr.write('warn-line\\n');
if (args.includes('--fail')) {
  process.stderr.write('boom\\n');
  process.exit(1);
}
if (args.includes('--sleep')) {
  setInterval(() => {}, 1000);
} else {
  process.exit(0);
}
`;

async function fixtureDir(): Promise<{ cwd: string; fixture: string }> {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-runtime-adapter-'));
  const fixture = path.join(cwd, 'fixture.cjs');
  await writeFile(fixture, FIXTURE, 'utf8');
  return { cwd, fixture };
}

test('resolveSpawnCommand uses defaults, config overrides, and output placeholder', () => {
  const defaultGo = resolveSpawnCommand(
    'go',
    'build',
    context('go', {}),
    params('/repo', { outputDir: 'out' }),
  );
  assert.deepEqual(defaultGo, {
    command: 'go',
    args: ['build', '-o', 'out', '.'],
    env: { OUTPUT_DIR: 'out' },
  });

  const override = resolveSpawnCommand(
    'go',
    'build',
    context('go', { runtimes: { go: { build: ['node', 'x.js', '{output}'] } } }),
    params('/repo', { outputDir: 'dist' }),
  );
  assert.deepEqual(override?.args, ['x.js', 'dist']);

  const missingPythonDev = resolveSpawnCommand(
    'python',
    'dev',
    context('python', {}),
    params('/repo'),
  );
  assert.equal(missingPythonDev, null);
});

test('go adapter runs build and reports success', async () => {
  const { cwd, fixture } = await fixtureDir();
  try {
    const adapter = resolveRuntimeAdapter('go');
    assert.ok(adapter);
    const execution = await adapter!.handleBuild(
      params(cwd),
      context('go', { runtimes: { go: { build: [process.execPath, fixture] } } }),
    );
    assert.ok(execution);
    assert.equal(execution!.result.success, true);
    assert.equal(execution!.result.runtime, 'go');
    assert.ok(execution!.events.some((event) => event.method === 'event.compiler_start'));
    assert.ok(execution!.events.some((event) => event.method === 'event.compiler_done'));
    assert.ok(
      execution!.events.some(
        (event) =>
          event.method === 'event.log' &&
          String((event.params as Record<string, unknown>).message).includes('started'),
      ),
    );
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('go adapter reports build failure with error event', async () => {
  const { cwd, fixture } = await fixtureDir();
  try {
    const adapter = resolveRuntimeAdapter('go');
    const execution = await adapter!.handleBuild(
      params(cwd),
      context('go', { runtimes: { go: { build: [process.execPath, fixture, '--fail'] } } }),
    );
    assert.equal(execution?.result.success, false);
    assert.equal(execution?.activeCompiler, undefined);
    assert.ok(
      execution!.events.some(
        (event) =>
          event.method === 'event.compiler_issue' &&
          (event.params as Record<string, unknown>).severity === 'error',
      ),
    );
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('go adapter fails fast when the toolchain is missing', async () => {
  const adapter = resolveRuntimeAdapter('go');
  const execution = await adapter!.handleBuild(
    params('/repo'),
    context('go', {
      runtimes: { go: { build: ['__lania_missing_bin__'] } },
    }),
  );
  assert.equal(execution?.result.success, false);
  assert.equal(execution?.activeCompiler, undefined);
});

test('go adapter rejects build watch mode', async () => {
  const adapter = resolveRuntimeAdapter('go');
  const execution = await adapter!.handleBuild(params('/repo', { watch: true }), context('go', {}));
  assert.equal(execution?.result.success, false);
  assert.ok(
    execution!.events.some(
      (event) =>
        event.method === 'event.compiler_issue' &&
        String((event.params as Record<string, unknown>).message).includes('watch not supported'),
    ),
  );
});

test('go adapter starts dev long task, injects env, and stops', async () => {
  const { cwd, fixture } = await fixtureDir();
  const logPath = path.join(cwd, 'fixture.json');
  process.env.FIXTURE_LOG = logPath;
  try {
    const adapter = resolveRuntimeAdapter('go');
    const execution = await adapter!.handleDev(
      params(cwd, { port: 4399, host: '0.0.0.0' }),
      context('go', {
        runtimes: { go: { dev: [process.execPath, fixture, '--sleep'] } },
      }),
    );
    assert.equal(execution?.result.longRunning, true);
    assert.equal(execution?.result.success, true);
    const active = execution?.activeCompiler;
    assert.equal(active?.tool, 'go');
    await active!.stop();

    const logged = JSON.parse(await readFile(logPath, 'utf8'));
    assert.equal(logged.port, '4399');
    assert.equal(logged.host, '0.0.0.0');
  } finally {
    delete process.env.FIXTURE_LOG;
    await rm(cwd, { recursive: true, force: true });
  }
});

test('compiler plugin routes a go project build through the runtime adapter', async () => {
  const { cwd, fixture } = await fixtureDir();
  try {
    await writeFile(path.join(cwd, 'go.mod'), 'module demo\n\ngo 1.22\n', 'utf8');
    await writeFile(
      path.join(cwd, 'lan.config.cjs'),
      `module.exports = { runtimes: { go: { build: ${JSON.stringify([
        process.execPath,
        fixture,
      ])} } } };`,
      'utf8',
    );

    const handled = await compilerPlugin.handle('compiler.build', { cwd, watch: false });
    const result = handled?.result as Record<string, unknown> | undefined;
    assert.equal(result?.success, true);
    assert.equal(result?.runtime, 'go');
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test('runtime adapter registry exposes go and excludes web', () => {
  assert.deepEqual(
    getRuntimeAdapters()
      .map((adapter) => adapter.runtime)
      .sort(),
    ['go'],
  );
  assert.equal(resolveRuntimeAdapter('web'), null);
});
