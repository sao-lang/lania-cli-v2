// 解析与发现类用例，覆盖动态命令树、prompt 元数据和 schema 转换。
import test from 'node:test';

import { loadRuntimeManifest } from '../dynamic-commands/parse-manifest.js';
import { assert, createDynamicCommandProject, handleExchange, rm, writeProjectFile } from './shared.js';

export function registerTests() {
  test('commands.resolveDynamic discovers manifest runtime commands', async (t) => {
    const cwd = await createDynamicCommandProject();
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{ name: string; subcommands: Array<{ name: string }> }>;
          handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }>;
        }
      | undefined;

    assert.equal(exchange.response.error, undefined);
    assert.equal(result?.commands[0]?.name, 'ops');
    assert.equal(
      result?.commands[0]?.subcommands.some((cmd) => cmd.name === 'ping'),
      true,
    );
    assert.equal(
      result?.handlers.some((handler) => handler.target.kind === 'manifest_command'),
      true,
    );
  });

  test('commands.resolveDynamic adapts author-side commands[] to top-level dynamic commands', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        commands: [
          {
            name: 'create',
            about: 'Create a project',
            subcommands: [
              {
                name: 'react-app',
                about: 'Create a React app',
                handler: async (ctx) => ({ result: { ok: true, input: ctx.argv.options, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-author-commands',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{ name: string; subcommands: Array<{ name: string }> }>;
          handlers: Array<{ target: { kind: string; path?: string[]; mount?: string } }>;
        }
      | undefined;

    const createCommand = result?.commands.find((command) => command.name === 'create');
    const manifestHandler = result?.handlers.find(
      (handler) => handler.target.kind === 'manifest_command',
    )?.target;

    assert.equal(exchange.response.error, undefined);
    assert.ok(createCommand);
    assert.equal(
      createCommand?.subcommands.some((command) => command.name === 'react-app'),
      true,
    );
    assert.equal(manifestHandler?.path?.join(' '), 'create react-app');
    assert.equal(manifestHandler?.mount, '__author__');
  });

  test('commands.resolveDynamic resolves command.workflow from workflows map', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        workflows: {
          createReactApp: async (ctx) => ({ result: { ok: true, from: ctx.path.join(' ') }, events: [] })
        },
        commands: [
          {
            name: 'create',
            subcommands: [
              {
                name: 'react-app',
                workflow: 'createReactApp'
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-workflow-resolve',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          handlers: Array<{ target: { kind: string; path?: string[] } }>;
          warnings?: string[];
        }
      | undefined;

    assert.equal(exchange.response.error, undefined);
    assert.equal(
      result?.handlers.some(
        (handler) =>
          handler.target.kind === 'manifest_command' &&
          handler.target.path?.join(' ') === 'create react-app',
      ),
      true,
    );
    assert.deepEqual(result?.warnings ?? [], []);
  });

  test('commands.resolveDynamic loads explicit schema.entry from lan.config', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        schema: {
          entry: './config/author.schemas.js'
        }
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'config/author.schemas.js',
      `export default {
        commands: [
          {
            name: 'author-create',
            handler: async (ctx) => ({ result: { ok: true, input: ctx.argv.options, exitCode: 0 }, events: [] })
          }
        ]
      };`,
    );

    const exchange = await handleExchange({
      id: 'req-schema-entry',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{ name: string }>;
          handlers: Array<{ target: { kind: string; path?: string[] } }>;
          warnings?: string[];
        }
      | undefined;

    assert.equal(exchange.response.error, undefined);
    assert.equal(result?.commands.some((command) => command.name === 'author-create'), true);
    assert.equal(
      result?.handlers.some(
        (handler) =>
          handler.target.kind === 'manifest_command' &&
          handler.target.path?.join(' ') === 'author-create',
      ),
      true,
    );
  });

  test('loadRuntimeManifest preserves top-level templates definitions', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        templates: [
          {
            id: 'react-vite',
            title: 'React + Vite',
            variables: [
              { key: 'projectName', type: 'string', required: true },
              { key: 'typescript', type: 'boolean', default: true }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const manifest = await loadRuntimeManifest(cwd, `${cwd}/lania.schemas.js`);

    assert.equal(manifest.templates.length, 1);
    assert.equal(manifest.templates[0]?.id, 'react-vite');
    assert.equal(manifest.templates[0]?.title, 'React + Vite');
    assert.equal(manifest.templates[0]?.variables?.[0]?.key, 'projectName');
  });

  test('loadRuntimeManifest preserves top-level plugins definitions', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        plugins: [
          '@acme/lania-plugin',
          {
            name: 'schema-converter',
            package: './plugins/schema.plugin.js',
            methods: ['commands.schemaToCommands']
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const manifest = await loadRuntimeManifest(cwd, `${cwd}/lania.schemas.js`);

    assert.equal(manifest.plugins.length, 2);
    assert.equal(manifest.plugins[0]?.package, '@acme/lania-plugin');
    assert.equal(manifest.plugins[1]?.name, 'schema-converter');
    assert.equal(manifest.plugins[1]?.methods?.[0], 'commands.schemaToCommands');
  });

  test('loadRuntimeManifest preserves high-level scaffolding schema definitions', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        commands: [
          {
            name: 'create-admin',
            promptFlow: 'createAdmin',
            preset: 'react-admin',
            features: ['typescript', 'router']
          }
        ],
        presets: {
          'react-admin': {
            templateLayers: ['admin-base', 'framework-react'],
            dependencyRecipes: ['react-core'],
            mergeRules: ['package-json-base'],
            guards: ['empty-dir']
          }
        },
        features: {
          typescript: {
            templateLayers: ['feature-typescript']
          }
        },
        dependencyRecipes: {
          'react-core': {
            dependencies: ['react', 'react-dom'],
            scripts: {
              dev: 'vite'
            }
          }
        },
        resolvers: {
          packageName: {
            from: 'name',
            use: 'kebabCase'
          }
        },
        mergeRules: {
          'package-json-base': {
            target: 'package.json',
            strategy: 'deep_merge'
          }
        },
        guards: {
          'empty-dir': {
            type: 'directory_empty'
          }
        },
        postActions: {
          installDeps: {
            type: 'install_dependencies'
          },
          gitInit: {
            type: 'git_init'
          }
        }
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const manifest = await loadRuntimeManifest(cwd, `${cwd}/lania.schemas.js`);

    assert.equal(manifest.commands[0]?.promptFlow, 'createAdmin');
    assert.equal(manifest.commands[0]?.preset, 'react-admin');
    assert.deepEqual(manifest.commands[0]?.features, ['typescript', 'router']);
    assert.deepEqual(manifest.presets['react-admin']?.templateLayers, [
      'admin-base',
      'framework-react',
    ]);
    assert.deepEqual(manifest.features.typescript?.templateLayers, ['feature-typescript']);
    assert.deepEqual(manifest.dependencyRecipes['react-core']?.dependencies, [
      'react',
      'react-dom',
    ]);
    assert.equal(manifest.dependencyRecipes['react-core']?.scripts?.dev, 'vite');
    assert.equal(manifest.mergeRules['package-json-base']?.strategy, 'deep_merge');
    assert.equal(manifest.guards['empty-dir']?.type, 'directory_empty');
    assert.equal(manifest.resolvers.packageName?.from, 'name');
    assert.equal(manifest.resolvers.packageName?.use, 'kebabCase');
    assert.equal(manifest.postActions.installDeps?.type, 'install_dependencies');
    assert.equal(manifest.postActions.gitInit?.type, 'git_init');
  });

  test('commands.resolveDynamic loads lan.config.json and supports prompt + requiredOptions', async (t) => {
    const cwd = await createDynamicCommandProject({
      configFileName: 'lan.config.json',
      configContent: JSON.stringify({
        extensions: { dynamicCommands: true },
        ui: { output: { mode: 'jsonl' } },
      }),
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            command: { about: 'Ops pack', alias: 'o' },
            commands: [
              {
                name: 'ping',
                about: 'Ping handler',
                alias: 'p',
                options: [{ long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true }],
                prompt: [{ field: 'endpoint', message: 'Endpoint?', kind: 'input', whenMissing: ['endpoint'] }],
                handler: async (ctx) => ({ result: { ok: true, input: ctx.argv.options, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-2',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{
            name: string;
            about: string;
            alias: string | null;
            subcommands: Array<{ name: string; alias: string | null }>;
          }>;
          handlers: Array<{
            target: { kind: string; requiredOptions?: string[]; prompt?: Array<{ field: string }> };
          }>;
        }
      | undefined;

    const mount = result?.commands[0];
    const cmd = mount?.subcommands[0];
    const handlerTarget = result?.handlers.find(
      (handler) => handler.target.kind === 'manifest_command',
    )?.target;

    assert.equal(exchange.response.error, undefined);
    assert.equal(mount?.about, 'Ops pack');
    assert.equal(mount?.alias, 'o');
    assert.equal(cmd?.name, 'ping');
    assert.equal(cmd?.alias, 'p');
    assert.deepEqual(handlerTarget?.requiredOptions, ['endpoint']);
    assert.equal(handlerTarget?.prompt?.[0]?.field, 'endpoint');
  });

  test('commands.resolveDynamic preserves advanced prompt flow fields', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'deploy',
                prompt: [
                  {
                    id: 'mode',
                    field: 'mode',
                    message: { zh: '模式？', en: 'Mode?' },
                    kind: 'select',
                    choices: [
                      { label: 'simple', value: 'simple' },
                      { label: 'advanced', value: 'advanced' }
                    ],
                    when: { type: 'truthy', key: 'project' },
                    goto: 'region',
                    validate: ['required', { type: 'one_of', values: ['simple', 'advanced'] }],
                    timeoutMs: 3000,
                    contextKey: 'deployMode',
                    accumulation: 'append',
                    returnable: true,
                    mapFunctions: ['trim', { type: 'lowercase' }],
                    onAnswered: [
                      { type: 'set_context_value', key: 'isAdvanced', value: true },
                      { type: 'goto_if', when: { type: 'truthy', key: 'isAdvanced' }, target: 'region' }
                    ]
                  }
                ],
                handler: async (ctx) => ({ result: { ok: true, input: ctx.argv.options, exitCode: 0 }, events: [] })
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const exchange = await handleExchange({
      id: 'req-advanced-prompt',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          handlers: Array<{
            target: {
              kind: string;
              prompt?: Array<Record<string, unknown>>;
            };
          }>;
        }
      | undefined;

    const prompt = result?.handlers.find((handler) => handler.target.kind === 'manifest_command')
      ?.target.prompt?.[0];
    assert.equal(exchange.response.error, undefined);
    assert.deepEqual(prompt?.message, { zh: '模式？', en: 'Mode?' });
    assert.deepEqual(prompt?.when, { type: 'truthy', key: 'project' });
    assert.equal(prompt?.goto, 'region');
    assert.deepEqual(prompt?.validate, [
      'required',
      { type: 'one_of', values: ['simple', 'advanced'] },
    ]);
    assert.equal(prompt?.timeoutMs, 3000);
    assert.equal(prompt?.contextKey, 'deployMode');
    assert.equal(prompt?.accumulation, 'append');
    assert.equal(prompt?.returnable, true);
    assert.deepEqual(prompt?.mapFunctions, ['trim', { type: 'lowercase' }]);
    assert.deepEqual(prompt?.onAnswered, [
      { type: 'set_context_value', key: 'isAdvanced', value: true },
      { type: 'goto_if', when: { type: 'truthy', key: 'isAdvanced' }, target: 'region' },
    ]);
  });

  test('commands.resolveDynamic supports schema -> commands via converter plugin', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        plugins: [{ package: './lania.plugin.js', methods: ['commands.schemaToCommands'] }]
      };`,
      pluginContent: `export default {
        name: 'schema-converter',
        methods: ['commands.schemaToCommands'],
        async handle(method, params) {
          if (method !== 'commands.schemaToCommands') return null;
          return {
            result: {
              commands: [
                {
                  name: 'from-schema',
                  about: 'Generated from schema',
                  handler: async (ctx) => ({ result: { ok: true, mount: params.mount, input: ctx.argv.options, exitCode: 0 }, events: [] })
                }
              ]
            },
            events: []
          };
        }
      };`,
      manifestContent: `export default {
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
      id: 'req-5',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{ name: string; subcommands: Array<{ name: string }> }>;
          handlers: Array<{ target: { kind: string; path?: string[] } }>;
        }
      | undefined;

    assert.equal(exchange.response.error, undefined);
    assert.equal(
      result?.commands[0]?.subcommands.some((cmd) => cmd.name === 'from-schema'),
      true,
    );
    assert.equal(
      result?.handlers.some(
        (handler) =>
          handler.target.kind === 'manifest_command' &&
          handler.target.path?.join(' ') === 'from-schema',
      ),
      true,
    );
  });

  test('commands.resolveDynamic supports schema converter plugin declared in schema plugins', async (t) => {
    const cwd = await createDynamicCommandProject({
      pluginContent: `export default {
        name: 'schema-converter',
        methods: ['commands.schemaToCommands'],
        async handle(method, params) {
          if (method !== 'commands.schemaToCommands') return null;
          return {
            result: {
              commands: [
                {
                  name: 'from-schema-plugins',
                  handler: async () => ({ result: { ok: true, exitCode: 0 }, events: [] })
                }
              ]
            },
            events: []
          };
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
      id: 'req-schema-plugin-manifest',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const result = exchange.response.result as
      | {
          commands: Array<{ name: string; subcommands: Array<{ name: string }> }>;
          handlers: Array<{ target: { kind: string; path?: string[] } }>;
        }
      | undefined;

    assert.equal(exchange.response.error, undefined);
    assert.equal(
      result?.commands[0]?.subcommands.some((cmd) => cmd.name === 'from-schema-plugins'),
      true,
    );
    assert.equal(
      result?.handlers.some(
        (handler) =>
          handler.target.kind === 'manifest_command' &&
          handler.target.path?.join(' ') === 'from-schema-plugins',
      ),
      true,
    );
  });
}
