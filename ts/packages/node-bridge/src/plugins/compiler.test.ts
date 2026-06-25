/**
 * compiler 插件的回归测试。
 *
 * 关键点：
 * - 包含文件系统读写/路径解析
 * - 包含 stdio 协议/流式读写
 * - 包含 JSON 协议/序列化
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import net from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { getCompilerAdapters } from './compiler-adapters/index.js';
import { compilerPlugin } from './compiler.js';
import { findAvailablePort } from './compiler-shared.js';

test('compiler adapters expose vite webpack and rollup', () => {
  assert.deepEqual(
    getCompilerAdapters()
      .map((adapter) => adapter.tool)
      .sort(),
    ['rollup', 'vite', 'webpack'],
  );
});

test('vite adapter handles dev runtime flow', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-vite-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify({ name: 'vite-app', dependencies: { vite: '^5.0.0' } }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = {
      buildTool: 'vite',
      buildAdaptors: {
        vite: {
          vite: {
            createServer: async (config) => ({
              listen: async () => {
                process.stdout.write('vite worker stdout\\n');
                process.stderr.write('vite worker stderr\\n');
              },
              close: async () => undefined,
              resolvedUrls: { local: [\`http://\${config.server.host}:\${config.server.port}\`] }
            })
          }
        }
      }
    };`,
  );
  await writeProjectFile(cwd, 'vite.config.cjs', 'module.exports = {};');

  const handled = (await compilerPlugin.handle('compiler.dev', {
    cwd,
    hmr: false,
    host: '127.0.0.1',
    mode: 'test',
    port: 4100,
  })) as
    | {
        result?: Record<string, unknown>;
        events?: Array<{ method: string; params?: Record<string, unknown> }>;
        activeCompiler?: { stop?: () => Promise<unknown> };
      }
    | undefined;
  const result = handled?.result as Record<string, any> | undefined;
  const events = handled?.events as Array<{ method: string; params: any }> | undefined;

  assert.equal(result?.implementation, 'runtime');
  assert.equal(result?.tool, 'vite');
  assert.equal(result?.mode, 'test');
  assert.equal(result?.eventSchema, 'lania.compiler.events.v1');
  assert.equal(result?.workerMode, 'isolated_worker');
  assert.ok(events?.some((event) => event.method === 'event.compiler_start'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_server_ready'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_done'));
  assert.ok(
    events?.some(
      (event) =>
        event.method === 'event.log' &&
        String(event.params?.message ?? '').includes('vite worker stdout'),
    ),
  );
  assert.ok(
    events?.some(
      (event) =>
        event.method === 'event.compiler_issue' &&
        String(event.params?.message ?? '').includes('vite worker stderr'),
    ),
  );
  assert.equal(
    events?.find((event) => event.method === 'event.dev_url')?.params.url,
    'http://127.0.0.1:4100',
  );

  await handled?.activeCompiler?.stop?.();
});

test('vite adapter forwards mode and hmr overrides into createServer config', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-vite-config-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  const capturePath = path.join(cwd, 'captured-vite-config.json');

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify({ name: 'vite-app', dependencies: { vite: '^5.0.0' } }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = { buildTool: 'vite' };`,
  );
  await writeProjectFile(
    cwd,
    'vite.config.cjs',
    'module.exports = { server: { hmr: true }, customKey: "preserved" };',
  );
  await writeProjectFile(
    cwd,
    'node_modules/vite/package.json',
    JSON.stringify({ name: 'vite', version: '5.0.0', main: 'index.js' }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'node_modules/vite/index.js',
    `const { writeFileSync } = require('node:fs');
    module.exports = {
      version: '5.0.0',
      createServer: async (config) => {
        writeFileSync(${JSON.stringify(capturePath)}, JSON.stringify(config, null, 2));
        return {
          listen: async () => undefined,
          close: async () => undefined,
          resolvedUrls: { local: [\`http://\${config.server.host}:\${config.server.port}\`] }
        };
      }
    };`,
  );

  const handled = (await compilerPlugin.handle('compiler.dev', {
    cwd,
    hmr: false,
    host: '127.0.0.1',
    mode: 'staging',
    port: 4200,
  })) as
    | {
        result?: Record<string, unknown>;
        activeCompiler?: { stop?: () => Promise<unknown> };
      }
    | undefined;

  const captured = JSON.parse(await readFile(capturePath, 'utf8'));

  assert.equal(handled?.result?.mode, 'staging');
  assert.equal(captured?.mode, 'staging');
  assert.equal(captured?.server?.hmr, false);
  assert.equal(captured?.server?.host, '127.0.0.1');
  assert.equal(captured?.server?.port, 4200);
  assert.equal(captured?.customKey, 'preserved');

  await handled?.activeCompiler?.stop?.();
});

test('findAvailablePort falls back to legacy safe range when 8089 is occupied', async (t) => {
  const occupied = net.createServer();
  // 8089 might already be taken by another local process in developer environments.
  // The core expectation is: when 8089 is not available, findAvailablePort falls back
  // into the historical safe range.
  const occupiedByTest = await new Promise<boolean>((resolve) => {
    occupied.once('listening', () => resolve(true));
    occupied.once('error', (error: any) => {
      resolve(error?.code !== 'EADDRINUSE' ? false : false);
    });
    occupied.listen(8089, '127.0.0.1');
  });
  if (occupiedByTest) {
    t.after(async () => {
      await new Promise<void>((resolve, reject) => {
        occupied.close((error) => (error ? reject(error) : resolve()));
      });
    });
  }

  const port = await findAvailablePort(8089, '127.0.0.1');

  assert.notEqual(port, 8089);
  assert.ok(port >= 18089 && port <= 18999);
});

test('findAvailablePort preserves custom-port intent before falling back broadly', async (t) => {
  const occupied = net.createServer();
  await new Promise<void>((resolve, reject) => {
    occupied.once('error', reject);
    occupied.listen(4310, '127.0.0.1', () => resolve());
  });
  t.after(async () => {
    await new Promise<void>((resolve, reject) => {
      occupied.close((error) => (error ? reject(error) : resolve()));
    });
  });

  const port = await findAvailablePort(4310, '127.0.0.1');

  assert.notEqual(port, 4310);
  assert.ok(port >= 4311);
  assert.ok(port <= 4330);
});

test('webpack dev reports a real error when webpack-dev-server is missing', async () => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-webpack-missing-dev-server-'));
  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify({ name: 'webpack-app', dependencies: { webpack: '^5.0.0' } }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = {
      buildTool: 'webpack',
      buildAdaptors: {
        webpack: {
          webpack: () => ({})
        }
      }
    };`,
  );
  await writeProjectFile(cwd, 'webpack.config.cjs', 'module.exports = { devServer: {} };');

  const handled = (await compilerPlugin.handle('compiler.dev', {
    cwd,
    host: '127.0.0.1',
    port: 4300,
  })) as
    | {
        result?: Record<string, unknown>;
        events?: Array<{ method: string; params?: Record<string, unknown> }>;
      }
    | undefined;
  const result = handled?.result as Record<string, any> | undefined;
  const events = handled?.events as Array<{ method: string; params?: Record<string, unknown> }> | undefined;

  assert.equal(result?.tool, 'webpack');
  assert.equal(result?.action, 'dev');
  assert.equal(result?.implementation, 'fallback');
  assert.equal(result?.longRunning, false);
  assert.ok(!events?.some((event) => event.method === 'event.compiler_server_ready'));
  assert.ok(!events?.some((event) => event.method === 'event.dev_url'));
  assert.ok(
    events?.some(
      (event) =>
        event.method === 'event.compiler_issue' &&
        event.params?.severity === 'error' &&
        String(event.params?.message ?? '').includes('webpack-dev-server'),
    ),
  );

  await rm(cwd, { recursive: true, force: true });
});

test('webpack dev accepts webpack-dev-server default export runtimes', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-webpack-default-export-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  const capturePath = path.join(cwd, 'captured-webpack-dev-server.json');

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify(
      {
        name: 'webpack-app',
        dependencies: {
          webpack: '^5.0.0',
          'webpack-dev-server': '^5.0.0',
        },
      },
      null,
      2,
    ),
  );
  await writeProjectFile(cwd, 'lan.config.cjs', `module.exports = { buildTool: 'webpack' };`);
  await writeProjectFile(cwd, 'webpack.config.cjs', 'module.exports = { devServer: {} };');
  await writeProjectFile(
    cwd,
    'node_modules/webpack/package.json',
    JSON.stringify({ name: 'webpack', version: '5.1.0', main: 'index.js' }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'node_modules/webpack/index.js',
    `function webpack(config) {
      return {
        config,
        close: (callback) => callback && callback(),
      };
    }
    webpack.version = '5.1.0';
    module.exports = webpack;`,
  );
  await writeProjectFile(
    cwd,
    'node_modules/webpack-dev-server/package.json',
    JSON.stringify({ name: 'webpack-dev-server', version: '5.0.0', main: 'index.js' }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'node_modules/webpack-dev-server/index.js',
    `const { writeFileSync } = require('node:fs');
    class FakeWebpackDevServer {
      constructor(options, compiler) {
        this.options = options;
        this.compiler = compiler;
      }
      async start() {
        writeFileSync(${JSON.stringify(capturePath)}, JSON.stringify({
          host: this.options.host,
          port: this.options.port,
          hasCompiler: !!this.compiler
        }, null, 2));
      }
      async stop() {}
    }
    module.exports = { default: FakeWebpackDevServer };`,
  );

  const handled = (await compilerPlugin.handle('compiler.dev', {
    cwd,
    host: '127.0.0.1',
    port: 4301,
  })) as
    | {
        result?: Record<string, unknown>;
        events?: Array<{ method: string; params?: Record<string, unknown> }>;
        activeCompiler?: { stop?: () => Promise<unknown> };
      }
    | undefined;
  const result = handled?.result as Record<string, any> | undefined;
  const events = handled?.events as Array<{ method: string; params?: Record<string, unknown> }> | undefined;
  const captured = JSON.parse(await readFile(capturePath, 'utf8'));

  assert.equal(result?.implementation, 'runtime');
  assert.equal(result?.tool, 'webpack');
  assert.equal(result?.workerMode, 'isolated_worker');
  assert.equal(captured.host, '127.0.0.1');
  assert.equal(captured.port, 4301);
  assert.equal(captured.hasCompiler, true);
  assert.ok(events?.some((event) => event.method === 'event.compiler_server_ready'));
  assert.equal(
    events?.find((event) => event.method === 'event.dev_url')?.params?.url,
    'http://127.0.0.1:4301',
  );

  await handled?.activeCompiler?.stop?.();
});

test('webpack adapter handles build runtime flow', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-webpack-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify({ name: 'webpack-app', dependencies: { webpack: '^5.0.0' } }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = {
      buildTool: 'webpack',
      buildAdaptors: {
        webpack: {
          webpack: () => ({
            run: (callback) => callback(null, {
              toJson: () => ({
                assets: [{ name: 'main.js', size: 1234 }],
                warnings: [],
                errors: []
              })
            }),
            close: (callback) => callback()
          })
        }
      }
    };`,
  );
  await writeProjectFile(
    cwd,
    'webpack.config.cjs',
    'module.exports = { output: { path: "dist" } };',
  );

  const handled = (await compilerPlugin.handle('compiler.build', { cwd })) as
    | {
        result?: Record<string, unknown>;
        events?: Array<{ method: string }>;
      }
    | undefined;
  const result = handled?.result as Record<string, any> | undefined;
  const events = handled?.events as Array<{ method: string }> | undefined;

  assert.equal(result?.implementation, 'runtime');
  assert.equal(result?.tool, 'webpack');
  assert.equal(result?.eventSchema, 'lania.compiler.events.v1');
  assert.equal(result?.workerMode, 'isolated_worker');
  assert.ok(events?.some((event) => event.method === 'event.compiler_start'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_asset'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_done'));
  assert.ok(events?.some((event) => event.method === 'event.build_asset'));
});

test('rollup adapter handles watch semantics', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-rollup-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await writeProjectFile(
    cwd,
    'package.json',
    JSON.stringify({ name: 'rollup-app', dependencies: { rollup: '^4.0.0' } }, null, 2),
  );
  await writeProjectFile(
    cwd,
    'lan.config.cjs',
    `module.exports = {
      buildTool: 'rollup',
      buildAdaptors: {
        rollup: {
          rollup: {
            rollup: async () => ({ write: async () => undefined }),
            watch: () => ({ close: () => undefined })
          }
        }
      }
    };`,
  );
  await writeProjectFile(
    cwd,
    'rollup.config.cjs',
    'module.exports = { output: { dir: "dist" } };',
  );

  const handled = (await compilerPlugin.handle('compiler.build', {
    cwd,
    watch: true,
  })) as
    | {
        result?: Record<string, unknown>;
        events?: Array<{ method: string }>;
        activeCompiler?: { stop?: () => Promise<unknown> };
      }
    | undefined;
  const result = handled?.result as Record<string, any> | undefined;
  const events = handled?.events as Array<{ method: string }> | undefined;

  assert.equal(result?.implementation, 'runtime');
  assert.equal(result?.longRunning, true);
  assert.equal(result?.eventSchema, 'lania.compiler.events.v1');
  assert.equal(result?.workerMode, 'isolated_worker');
  assert.ok(events?.some((event) => event.method === 'event.compiler_start'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_status'));
  assert.ok(events?.some((event) => event.method === 'event.compiler_done'));

  await handled?.activeCompiler?.stop?.();
});

async function writeProjectFile(cwd: string, relativePath: string, content: string) {
  const filePath = path.join(cwd, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}
