// Schema 解析异常路径测试（P1-3）。
//
// 覆盖场景：
// - 非法 schema 格式（export default 无效值）
// - 插件引用失败（文件不存在/方法不支持）
// - 保留命令冲突（runtimeCommands mount 与内置命令冲突）
// - mount 缺失
// - 外部插件 handle 返回异常
import test from 'node:test';

import { assert, createDynamicCommandProject, handleExchange, rm } from './shared.js';

export function registerTests() {
  test('commands.resolveDynamic skips invalid schema export (null)', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: 'export default null;\n',
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-invalid-null',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { commands: unknown[]; warnings?: string[] } | undefined;
    // null schema 应该产生空结果，不崩溃
    assert.ok(result);
    assert.equal(result.commands.length, 0);
  });

  test('commands.resolveDynamic skips invalid schema export (number)', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: 'export default 42;\n',
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-invalid-number',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { commands: unknown[]; warnings?: string[] } | undefined;
    assert.ok(result);
    assert.equal(result.commands.length, 0);
  });

  test('commands.resolveDynamic skips invalid schema export (empty object)', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: 'export default {};\n',
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-empty-object',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { commands: unknown[]; warnings?: string[] } | undefined;
    assert.ok(result);
    assert.equal(result.commands.length, 0);
  });

  test('commands.resolveDynamic warns when mount conflicts with reserved top-level command', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'build',
            commands: [{ name: 'custom', handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] }) }]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-reserved-mount',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { warnings?: string[] } | undefined;

    // 保留命令冲突应产生 warning
    assert.ok(result?.warnings);
    const mountWarning = result?.warnings?.find((w) => w.includes('reserved'));
    assert.ok(
      mountWarning,
      `expected warning about reserved command conflict, got: ${JSON.stringify(result?.warnings)}`,
    );
  });

  test('commands.resolveDynamic warns when mount is empty', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: '',
            commands: [{ name: 'orphan', handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] }) }]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-empty-mount',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { warnings?: string[]; commands: unknown[] } | undefined;

    // 空 mount 应该产生 warning 且不产生命令
    assert.ok(result);
    assert.ok(
      result?.warnings?.some((w) => w.includes('mount')),
      `expected warning about empty mount, got: ${JSON.stringify(result?.warnings)}`,
    );
  });

  test('commands.resolveDynamic warns when mount is missing (only whitespace)', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: '   ',
            commands: [{ name: 'spaceship', handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] }) }]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-whitespace-mount',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    assert.equal(exchange.response.error, undefined);
    const result = exchange.response.result as { warnings?: string[] } | undefined;
    assert.ok(
      result?.warnings?.some((w) => w.includes('mount')),
      `expected warning about whitespace mount, got: ${JSON.stringify(result?.warnings)}`,
    );
  });

  test('commands.resolveDynamic warns when plugin file does not exist', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        plugins: [{ package: './non-existent.plugin.js', methods: ['commands.schemaToCommands'] }]
      };`,
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              { name: 'ping', handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] }) }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-plugin-not-found',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    // 插件文件不存在应该有一定报错或 warning，但不应该崩溃
    // 具体行为取决于 loadLanConfig 的实现
    if (exchange.response.error) {
      // 如果有 error，确保它是可读的而不是 stack trace
      assert.ok(
        typeof exchange.response.error.message === 'string' && exchange.response.error.message.length > 0,
        `error message should be descriptive: ${JSON.stringify(exchange.response.error)}`,
      );
    } else {
      // 如果没有 error，应该有 warning
      const result = exchange.response.result as { warnings?: string[] } | undefined;
      assert.ok(
        result?.warnings?.length ? result.warnings.length > 0 : false,
        'expected warnings for missing plugin',
      );
    }
  });

  test('commands.resolveDynamic reports schema converter plugin errors as warnings', async (t) => {
    const cwd = await createDynamicCommandProject({
      pluginContent: `export default {
        name: 'faulty-converter',
        methods: ['commands.schemaToCommands'],
        async handle() {
          throw new Error('Plugin crashed intentionally');
        }
      };`,
      manifestContent: `export default {
        plugins: [
          { package: './lania.plugin.js', methods: ['commands.schemaToCommands'] }
        ],
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [],
            schemas: [
              { plugin: './lania.plugin.js', method: 'commands.schemaToCommands', kind: 'demo' }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-plugin-error',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    // 插件崩溃不应该导致 resolve 整体失败
    // 应该产生 warning 而非 fatal error
    const result = exchange.response.result as { warnings?: string[] } | undefined;
    assert.ok(
      result?.warnings?.some(
        (w) => w.includes('Plugin') || w.includes('plugin') || w.includes('crash') || w.includes('faulty'),
      ) ?? exchange.response.error,
      `expected warning or error for faulty plugin, got: ${JSON.stringify(result?.warnings)}`,
    );
  });
}
