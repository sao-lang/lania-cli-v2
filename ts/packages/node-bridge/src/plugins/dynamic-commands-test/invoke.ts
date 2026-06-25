// 执行类用例，覆盖 manifest handler 调用和 schema handler 的工具注入。
import test from 'node:test';

import {
  assert,
  createHostRpcResponder,
  createDynamicCommandProject,
  handleExchange,
  installHostRpcTransport,
  readFile,
  resetHostRpcTransport,
  rm,
  respondUnsupportedTestHostMethod,
  writeProjectFile,
  writeFile,
} from './shared.js';

export function registerTests() {
  test('command.invokeDynamic executes manifest command handler', async (t) => {
    const cwd = await createDynamicCommandProject();
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-3',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' && entry.target.path?.join(' ') === 'ping',
    );
    assert.ok(handler, 'expected manifest command handler');

    const invocation = await handleExchange({
      id: 'req-4',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: { endpoint: 'http://127.0.0.1:1' } },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | { exitCode: number; result: { ok: boolean; input: { endpoint: string } } }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.exitCode, 0);
    assert.equal(result?.result.ok, true);
    assert.equal(result?.result.input.endpoint, 'http://127.0.0.1:1');
  });

  test('command.invokeDynamic injects ctx.tools for schema handlers', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'inspect',
                handler: async (ctx) => {
                  const pkgPath = ctx.tools.path.resolve(ctx.cwd, 'package.json');
                  const pkg = await ctx.tools.workspace.packageJson();
                  const manager = await ctx.tools.workspace.detectPackageManager();
                  const lint = await ctx.tools.bridge.commit.commitlint('feat(core): short');
                  return ctx.tools.result.ok({
                    pkgPath,
                    packageName: pkg?.name ?? null,
                    manager,
                    lintValid: lint.response.result.valid
                  });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'package.json',
      JSON.stringify({ name: 'schema-tools-demo' }, null, 2),
    );
    await writeProjectFile(cwd, 'pnpm-lock.yaml', 'lockfileVersion: 9.0');

    const resolved = await handleExchange({
      id: 'req-tools-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' && entry.target.path?.join(' ') === 'inspect',
    );
    assert.ok(handler, 'expected inspect handler');

    const invocation = await handleExchange({
      id: 'req-tools-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            data?: { pkgPath: string; packageName: string; manager: string; lintValid: boolean };
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.result.data?.packageName, 'schema-tools-demo');
    assert.equal(result?.result.data?.manager, 'pnpm');
    assert.equal(result?.result.data?.lintValid, true);
    assert.ok(result?.result.data?.pkgPath.endsWith('package.json'));
  });

  test('command.invokeDynamic injects ctx.product and ctx.runtime', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          displayName: 'Acme CLI',
          version: '1.2.3',
          templatesDir: './templates'
        },
        schema: {
          entry: './config/author.schemas.js'
        }
      };`,
      manifestFileName: 'config/author.schemas.js',
      manifestContent: `export default {
        commands: [
          {
            name: 'inspect-context',
            handler: async (ctx) => ({
              result: {
                product: ctx.product,
                runtime: ctx.runtime,
                exitCode: 0
              },
              events: []
            })
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(cwd, 'templates/.gitkeep', '');

    const resolved = await handleExchange({
      id: 'req-context-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });

    const resolveResult = resolved.response.result as
      | {
          handlers: Array<{
            handlerId: string;
            target: { kind: string; path?: string[]; schemaRoot?: string };
          }>;
        }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'inspect-context',
    );
    assert.ok(handler, 'expected inspect-context handler');

    const invocation = await handleExchange({
      id: 'req-context-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        traceId: 'trace-context',
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            product?: Record<string, unknown>;
            runtime?: Record<string, unknown>;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.result.product?.name, '@acme/cli');
    assert.equal(result?.result.product?.binaryName, 'acme');
    assert.equal(result?.result.product?.displayName, 'Acme CLI');
    assert.equal(result?.result.product?.version, '1.2.3');
    assert.equal(
      result?.result.product?.templatesDir,
      `${cwd}/templates`,
    );
    assert.equal(result?.result.runtime?.mode, 'development');
    assert.equal(result?.result.runtime?.traceId, 'trace-context');
    assert.equal(result?.result.runtime?.invocationCwd, cwd);
    assert.equal(result?.result.runtime?.workspaceRoot, cwd);
    assert.equal(result?.result.runtime?.productRoot, cwd);
    assert.equal(result?.result.runtime?.schemaRoot, `${cwd}/config`);
  });

  test('command.invokeDynamic distinguishes installed mode productRoot and workspaceRoot', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          version: '2.0.0',
          templatesDir: './templates'
        },
        schema: {
          entry: './config/author.schemas.js'
        }
      };`,
      manifestFileName: 'config/author.schemas.js',
      manifestContent: `export default {
        commands: [
          {
            name: 'inspect-installed-context',
            handler: async (ctx) => ({
              result: {
                product: ctx.product,
                runtime: ctx.runtime,
                exitCode: 0
              },
              events: []
            })
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const installRoot = `${cwd}/install-root`;
    await writeProjectFile(
      installRoot,
      'lan.config.js',
      `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          version: '2.0.0',
          templatesDir: './templates'
        },
        schema: {
          entry: './config/author.schemas.js'
        }
      };`,
    );
    await writeProjectFile(
      installRoot,
      'config/author.schemas.js',
      `export { default } from ${JSON.stringify(`${cwd}/config/author.schemas.js`)};`,
    );
    await writeProjectFile(installRoot, 'templates/.gitkeep', '');

    const resolved = await handleExchange({
      id: 'req-installed-context-1',
      method: 'commands.resolveDynamic',
      params: {
        cwd,
        productRoot: installRoot,
        runtimeMode: 'installed',
      },
    });

    const resolveResult = resolved.response.result as
      | {
          handlers: Array<{
            handlerId: string;
            target: { kind: string; path?: string[]; schemaRoot?: string };
          }>;
        }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'inspect-installed-context',
    );
    assert.ok(handler, 'expected inspect-installed-context handler');

    const invocation = await handleExchange({
      id: 'req-installed-context-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        workspaceRoot: cwd,
        productRoot: installRoot,
        runtimeMode: 'installed',
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        traceId: 'trace-installed',
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            product?: Record<string, unknown>;
            runtime?: Record<string, unknown>;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.result.runtime?.mode, 'installed');
    assert.equal(result?.result.runtime?.traceId, 'trace-installed');
    assert.equal(result?.result.runtime?.invocationCwd, cwd);
    assert.equal(result?.result.runtime?.workspaceRoot, cwd);
    assert.equal(result?.result.runtime?.productRoot, installRoot);
    assert.equal(result?.result.runtime?.schemaRoot, `${installRoot}/config`);
    assert.equal(result?.result.product?.productRoot, installRoot);
    assert.equal(result?.result.product?.schemaRoot, `${installRoot}/config`);
    assert.equal(result?.result.product?.templatesDir, `${installRoot}/templates`);
  });

  test('command.invokeDynamic resolves ctx.product.version from package.json versionStrategy', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          versionStrategy: 'package_json'
        },
        schema: {
          entry: './config/author.schemas.js'
        }
      };`,
      manifestFileName: 'config/author.schemas.js',
      manifestContent: `export default {
        commands: [
          {
            name: 'inspect-version',
            handler: async (ctx) => ({
              result: {
                version: ctx.product.version,
                exitCode: 0
              },
              events: []
            })
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'package.json',
      JSON.stringify({ name: '@acme/cli', version: '9.9.9' }, null, 2),
    );

    const resolved = await handleExchange({
      id: 'req-version-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'inspect-version',
    );
    assert.ok(handler, 'expected inspect-version handler');

    const invocation = await handleExchange({
      id: 'req-version-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            version?: string | null;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.result.version, '9.9.9');
  });

  test('command.invokeDynamic executes workflow referenced by command.workflow', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        workflows: {
          createReactApp: async (ctx) => ({
            result: {
              ok: true,
              commandPath: ctx.path.join(' '),
              projectName: ctx.argv.options.name ?? null,
              binaryName: ctx.product.binaryName
            },
            events: []
          })
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
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme'
        }
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-workflow-invoke-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'create react-app',
    );
    assert.ok(handler, 'expected workflow-backed handler');

    const invocation = await handleExchange({
      id: 'req-workflow-invoke-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: { name: 'demo-app' } },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          exitCode: number;
          result: {
            ok: boolean;
            commandPath: string;
            projectName: string;
            binaryName: string;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.exitCode, 0);
    assert.equal(result?.result.ok, true);
    assert.equal(result?.result.commandPath, 'create react-app');
    assert.equal(result?.result.projectName, 'demo-app');
    assert.equal(result?.result.binaryName, 'acme');
  });

  test('command.workflow receives resolved scaffold plan from presets and features', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        workflows: {
          createAdmin: async (ctx) => ({
            result: {
              scaffold: ctx.scaffold,
              exitCode: 0
            },
            events: []
          })
        },
        presets: {
          reactAdmin: {
            templateLayers: ['admin-base'],
            features: ['typescript'],
            dependencyRecipes: ['react-core'],
            mergeRules: ['package-json-base'],
            guards: ['workspace-empty'],
            postActions: ['installDeps', 'gitInit', 'printSummary']
          }
        },
        features: {
          typescript: {
            templateLayers: ['feature-typescript'],
            dependencyRecipes: ['ts-tooling']
          },
          router: {
            templateLayers: ['feature-router'],
            dependencyRecipes: ['router-core'],
            when: { field: 'options.router', truthy: true }
          }
        },
        dependencyRecipes: {
          'react-core': {
            dependencies: ['react', 'react-dom'],
            scripts: { dev: 'vite' }
          },
          'ts-tooling': {
            devDependencies: ['typescript', '@types/node'],
            packageManager: 'pnpm'
          },
          'router-core': {
            dependencies: ['react-router-dom']
          }
        },
        mergeRules: {
          'package-json-base': {
            target: 'package.json',
            strategy: 'deep_merge'
          }
        },
        guards: {
          'workspace-empty': {
            type: 'directory_empty'
          }
        },
        resolvers: {
          packageName: {
            from: 'name',
            use: 'kebabCase'
          }
        },
        postActions: {
          installDeps: {
            type: 'install_dependencies'
          },
          gitInit: {
            type: 'git_init'
          },
          printSummary: {
            type: 'print_summary'
          }
        },
        commands: [
          {
            name: 'create-admin',
            preset: 'reactAdmin',
            features: ['router'],
            workflow: 'createAdmin'
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const resolved = await handleExchange({
      id: 'req-scaffold-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'create-admin',
    );
    assert.ok(handler, 'expected create-admin handler');

    const invocation = await handleExchange({
      id: 'req-scaffold-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: { router: true } },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            scaffold?: {
              preset?: string | null;
              features?: string[];
              templateLayers?: string[];
              dependencyRecipes?: string[];
              dependencies?: string[];
              devDependencies?: string[];
              scripts?: Record<string, string>;
              packageManager?: string | null;
              resolvers?: Record<string, { from: string; use: string }>;
              mergeRules?: Array<{ name: string; target: string; strategy: string }>;
              guards?: Array<{ name: string; type: string }>;
              postActions?: Array<{ name: string; type: string }>;
            };
          };
        }
      | undefined;

    const scaffold = result?.result.scaffold;
    assert.equal(invocation.response.error, undefined);
    assert.equal(scaffold?.preset, 'reactAdmin');
    assert.deepEqual(scaffold?.features, ['typescript', 'router']);
    assert.deepEqual(scaffold?.templateLayers, [
      'admin-base',
      'feature-typescript',
      'feature-router',
    ]);
    assert.deepEqual(scaffold?.dependencyRecipes, ['react-core', 'ts-tooling', 'router-core']);
    assert.deepEqual(scaffold?.dependencies, ['react', 'react-dom', 'react-router-dom']);
    assert.deepEqual(scaffold?.devDependencies, ['typescript', '@types/node']);
    assert.equal(scaffold?.scripts?.dev, 'vite');
    assert.equal(scaffold?.packageManager, 'pnpm');
    assert.deepEqual(scaffold?.resolvers?.packageName, {
      from: 'name',
      use: 'kebabCase',
    });
    assert.deepEqual(scaffold?.mergeRules, [
      { name: 'package-json-base', target: 'package.json', strategy: 'deep_merge' },
    ]);
    assert.deepEqual(scaffold?.guards, [{ name: 'workspace-empty', type: 'directory_empty' }]);
    assert.deepEqual(scaffold?.postActions, [
      { name: 'installDeps', type: 'install_dependencies' },
      { name: 'gitInit', type: 'git_init' },
      { name: 'printSummary', type: 'print_summary' },
    ]);
  });

  test('command.invokeDynamic exposes tools.scaffold for template layers and dependency plan', async (t) => {
    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          templatesDir: './templates'
        }
      };`,
      manifestContent: `export default {
        presets: {
          webapp: {
            templateLayers: ['base-app'],
            features: ['typescript'],
            dependencyRecipes: ['react-core'],
            mergeRules: ['tsconfig-merge'],
            guards: ['node-ready'],
            postActions: ['install-if-enabled', 'git-init-if-enabled', 'summary']
          }
        },
        features: {
          typescript: {
            templateLayers: ['ts-base'],
            dependencyRecipes: ['ts-tooling']
          },
          auth: {
            templateLayers: ['feature-auth'],
            dependencyRecipes: ['auth-core'],
            when: { field: 'options.auth', truthy: true }
          }
        },
        dependencyRecipes: {
          'react-core': {
            dependencies: ['react', 'react-dom'],
            scripts: { dev: 'vite' }
          },
          'ts-tooling': {
            devDependencies: ['typescript'],
            packageManager: 'pnpm'
          },
          'auth-core': {
            dependencies: ['zod']
          }
        },
        commands: [
          {
            name: 'create-webapp',
            preset: 'webapp',
            features: ['auth'],
            handler: async (ctx) => {
              const rendered = await ctx.tools.scaffold.renderTemplateLayers({
                context: { projectName: 'demo-kit' },
                options: { auth: true }
              });
              const plan = await ctx.tools.scaffold.dependencyPlan({
                manager: 'pnpm',
                context: { projectName: 'demo-kit' },
                options: { auth: true }
              });
              return {
                result: {
                  currentPlan: ctx.tools.scaffold.currentPlan(),
                  rendered,
                  plan,
                  exitCode: 0
                },
                events: []
              };
            }
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'templates/base-app/template.json',
      JSON.stringify(
        {
          name: 'base-app',
          schemaVersion: 1,
          renderEngine: 'node_bridge',
          legacyTemplateDir: 'base-app',
          ownership: 'third_party_extension',
          useCases: ['create'],
        },
        null,
        2,
      ),
    );
    await writeProjectFile(
      cwd,
      'templates/base-app/files/package.json.ejs',
      '{\n  "name": "<%= projectName %>",\n  "packageManager": "<%= packageManager %>"\n}\n',
    );
    await writeProjectFile(
      cwd,
      'templates/ts-base/template.json',
      JSON.stringify(
        {
          name: 'ts-base',
          schemaVersion: 1,
          renderEngine: 'node_bridge',
          legacyTemplateDir: 'ts-base',
          ownership: 'third_party_extension',
          useCases: ['create'],
        },
        null,
        2,
      ),
    );
    await writeProjectFile(
      cwd,
      'templates/ts-base/files/tsconfig.json.ejs',
      '{\n  "extends": "./tsconfig.base.json"\n}\n',
    );
    await writeProjectFile(
      cwd,
      'templates/feature-auth/template.json',
      JSON.stringify(
        {
          name: 'feature-auth',
          schemaVersion: 1,
          renderEngine: 'node_bridge',
          legacyTemplateDir: 'feature-auth',
          ownership: 'third_party_extension',
          useCases: ['create'],
        },
        null,
        2,
      ),
    );
    await writeProjectFile(
      cwd,
      'templates/feature-auth/files/src/auth.ts.ejs',
      'export const authEnabled = true;\n',
    );

    const resolved = await handleExchange({
      id: 'req-scaffold-tools-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'create-webapp',
    );
    assert.ok(handler, 'expected create-webapp handler');

    const invocation = await handleExchange({
      id: 'req-scaffold-tools-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: { auth: true } },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          result: {
            currentPlan?: { templateLayers?: string[] };
            rendered?: {
              templates?: string[];
              files?: Array<{ path: string; content: string }>;
              collisions?: string[];
            };
            plan?: {
              manager?: string;
              templates?: string[];
              dependencies?: string[];
              devDependencies?: string[];
              scripts?: Record<string, string>;
              packageJsonPatch?: {
                dependencies?: Record<string, string>;
                devDependencies?: Record<string, string>;
                scripts?: Record<string, string>;
              };
              installCommands?: Array<{ program: string; args: string[] }>;
            };
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.deepEqual(result?.result.currentPlan?.templateLayers, [
      'base-app',
      'ts-base',
      'feature-auth',
    ]);
    assert.deepEqual(result?.result.rendered?.templates, [
      'base-app',
      'ts-base',
      'feature-auth',
    ]);
    assert.deepEqual(
      result?.result.rendered?.files?.map((file) => file.path).sort(),
      ['package.json', 'src/auth.ts', 'tsconfig.json'],
    );
    assert.deepEqual(result?.result.rendered?.collisions, []);
    assert.equal(result?.result.plan?.manager, 'pnpm');
    assert.deepEqual(result?.result.plan?.templates, ['base-app', 'ts-base', 'feature-auth']);
    assert.deepEqual(result?.result.plan?.dependencies, ['react', 'react-dom', 'zod']);
    assert.deepEqual(result?.result.plan?.devDependencies, ['typescript']);
    assert.equal(result?.result.plan?.scripts?.dev, 'vite');
    assert.equal(result?.result.plan?.packageJsonPatch?.dependencies?.react, 'latest');
    assert.equal(result?.result.plan?.packageJsonPatch?.devDependencies?.typescript, 'latest');
    assert.equal(result?.result.plan?.packageJsonPatch?.scripts?.dev, 'vite');
    assert.deepEqual(result?.result.plan?.installCommands, [
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', 'react', 'react-dom', 'zod'],
      },
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', '--save-dev', 'typescript'],
      },
    ]);
  });

  test('command.invokeDynamic executes named workflow object definitions', async (t) => {
    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        workflows: {
          localRef: {
            name: 'createReactApp',
            handler: async (ctx) => ({
              result: {
                ok: true,
                workflowName: 'createReactApp',
                commandPath: ctx.path.join(' ')
              },
              events: []
            })
          }
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

    const resolved = await handleExchange({
      id: 'req-named-workflow-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'create react-app',
    );
    assert.ok(handler, 'expected named workflow-backed handler');

    const invocation = await handleExchange({
      id: 'req-named-workflow-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          exitCode: number;
          result: {
            ok: boolean;
            workflowName: string;
            commandPath: string;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.exitCode, 0);
    assert.equal(result?.result.ok, true);
    assert.equal(result?.result.workflowName, 'createReactApp');
    assert.equal(result?.result.commandPath, 'create react-app');
  });

  test('command.invokeDynamic executes declarative workflow steps', async (t) => {
    const execCalls: Array<{ program?: string; args?: string[]; cwd?: string }> = [];
    const logMessages: string[] = [];
    let gitInitialized = false;

    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as { id: string; method: string; params: Record<string, unknown> };
        const respond = createHostRpcResponder(payload);
        const method = payload.method;
        if (method === 'host.fs.exists') {
          const path = `${String(payload.params.cwd ?? '')}/${String(payload.params.path ?? '')}`.replace(/\/+/g, '/');
          try {
            await readFile(path, 'utf8');
            respond({ exists: true });
          } catch {
            respond({ exists: false });
          }
          return;
        }
        if (method === 'host.fs.read') {
          const path = `${String(payload.params.cwd ?? '')}/${String(payload.params.path ?? '')}`.replace(/\/+/g, '/');
          respond({ content: await readFile(path, 'utf8') });
          return;
        }
        if (method === 'host.fs.write') {
          const path = `${String(payload.params.cwd ?? '')}/${String(payload.params.path ?? '')}`.replace(/\/+/g, '/');
          await writeFile(path, String(payload.params.content ?? ''));
          respond({ ok: true });
          return;
        }
        if (method === 'host.fs.mkdirp') {
          respond({ ok: true });
          return;
        }
        if (method === 'host.exec.runChecked') {
          execCalls.push({
            program: typeof payload.params.program === 'string' ? payload.params.program : undefined,
            args: Array.isArray(payload.params.args)
              ? payload.params.args.map((entry) => String(entry))
              : undefined,
            cwd: typeof payload.params.cwd === 'string' ? payload.params.cwd : undefined,
          });
          if (
            payload.params.program === 'node' &&
            Array.isArray(payload.params.args) &&
            payload.params.args.length === 1 &&
            payload.params.args[0] === '--version'
          ) {
            respond({
              exitCode: 0,
              stdout: 'v20.10.0\n',
              stderr: '',
              skipped: false,
              timedOut: false,
              cancelled: false,
            });
            return;
          }
          respond({
            exitCode: 0,
            stdout: JSON.stringify(payload.params),
            stderr: '',
            skipped: false,
            timedOut: false,
            cancelled: false,
          });
          return;
        }
        if (method === 'host.git.git.isInit') {
          respond({ isInit: gitInitialized });
          return;
        }
        if (method === 'host.git.git.init') {
          gitInitialized = true;
          respond({ ok: true });
          return;
        }
        if (method === 'host.git.plan.init') {
          respond({ program: 'git', args: ['init'] });
          return;
        }
        if (method === 'host.log.emit') {
          logMessages.push(String(payload.params.message ?? ''));
          respond({ ok: true });
          return;
        }
        respondUnsupportedTestHostMethod(payload);
      },
    });
    t.after(() => resetHostRpcTransport());

    const cwd = await createDynamicCommandProject({
      configContent: `export default {
        extensions: { dynamicCommands: true },
        product: {
          name: '@acme/cli',
          binaryName: 'acme',
          templatesDir: './templates'
        }
      };`,
      manifestContent: `export default {
        presets: {
          webapp: {
            templateLayers: ['base-app'],
            features: ['typescript'],
            dependencyRecipes: ['react-core'],
            mergeRules: ['tsconfig-merge'],
            guards: ['node-ready'],
            postActions: ['install-if-enabled', 'git-init-if-enabled', 'summary']
          }
        },
        features: {
          typescript: {
            templateLayers: ['ts-base'],
            dependencyRecipes: ['ts-tooling']
          }
        },
        dependencyRecipes: {
          'react-core': {
            dependencies: ['react'],
            scripts: { dev: 'vite' }
          },
          'ts-tooling': {
            devDependencies: ['typescript'],
            packageManager: 'pnpm'
          }
        },
        mergeRules: {
          'tsconfig-merge': {
            target: 'tsconfig.json',
            strategy: 'deep_merge'
          }
        },
        guards: {
          'node-ready': {
            type: 'node_version',
            range: '>=18'
          }
        },
        postActions: {
          'install-if-enabled': {
            type: 'install_dependencies'
          },
          'git-init-if-enabled': {
            type: 'git_init'
          },
          summary: {
            type: 'print_summary'
          }
        },
        workflows: {
          scaffoldWebapp: {
            guards: ['node-ready'],
            steps: [
              'preflight',
              'resolvePreset',
              'resolveFeatures',
              {
                name: 'renderTemplates',
                options: {
                  context: { projectName: 'workflow-demo' }
                }
              },
              'mergeFiles',
              'writeFiles',
              {
                name: 'installDependencies',
                options: {
                  manager: 'pnpm',
                  context: { projectName: 'workflow-demo' }
                }
              },
              'gitInit',
              {
                name: 'postActions',
                options: {
                  execute: false,
                  emit: false,
                  manager: 'pnpm',
                  context: { projectName: 'workflow-demo' }
                }
              },
              'printSummary'
            ]
          }
        },
        commands: [
          {
            name: 'create-webapp',
            preset: 'webapp',
            workflow: 'scaffoldWebapp'
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'templates/base-app/template.json',
      JSON.stringify(
        {
          name: 'base-app',
          schemaVersion: 1,
          renderEngine: 'node_bridge',
          legacyTemplateDir: 'base-app',
          ownership: 'third_party_extension',
          useCases: ['create'],
        },
        null,
        2,
      ),
    );
    await writeProjectFile(
      cwd,
      'templates/base-app/files/package.json.ejs',
      '{\n  "name": "<%= projectName %>"\n}\n',
    );
    await writeProjectFile(
      cwd,
      'templates/ts-base/template.json',
      JSON.stringify(
        {
          name: 'ts-base',
          schemaVersion: 1,
          renderEngine: 'node_bridge',
          legacyTemplateDir: 'ts-base',
          ownership: 'third_party_extension',
          useCases: ['create'],
        },
        null,
        2,
      ),
    );
    await writeProjectFile(
      cwd,
      'templates/ts-base/files/tsconfig.json.ejs',
      '{\n  "compilerOptions": {\n    "strict": true\n  }\n}\n',
    );
    await writeProjectFile(
      cwd,
      'tsconfig.json',
      '{\n  "compilerOptions": {\n    "target": "ES2022"\n  }\n}\n',
    );

    const resolved = await handleExchange({
      id: 'req-declarative-workflow-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'create-webapp',
    );
    assert.ok(handler, 'expected declarative workflow handler');

    const invocation = await handleExchange({
      id: 'req-declarative-workflow-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: {} },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          exitCode: number;
          result: {
            steps?: Array<{ step: string; ok: boolean; data?: Record<string, unknown> }>;
            summary?: {
              templateLayers?: string[];
              dependencies?: string[];
              devDependencies?: string[];
              scripts?: Record<string, string>;
              packageManager?: string | null;
              nextSteps?: string[];
            };
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.exitCode, 0);
    assert.deepEqual(
      result?.result.steps?.map((step) => step.step),
      [
        'preflight',
        'resolvePreset',
        'resolveFeatures',
        'renderTemplates',
        'mergeFiles',
        'writeFiles',
        'installDependencies',
        'gitInit',
        'postActions',
        'printSummary',
      ],
    );
    const preflightGuards = result?.result.steps?.[0]?.data?.guards as
      | Array<{ name?: string; type?: string; ok?: boolean }>
      | undefined;
    assert.equal(preflightGuards?.length, 1);
    assert.equal(preflightGuards?.[0]?.name, 'node-ready');
    assert.equal(preflightGuards?.[0]?.type, 'node_version');
    assert.equal(preflightGuards?.[0]?.ok, true);
    assert.equal(result?.result.steps?.[1]?.data?.preset, 'webapp');
    assert.deepEqual(result?.result.steps?.[2]?.data?.features, ['typescript']);
    assert.deepEqual(result?.result.steps?.[3]?.data?.templates, ['base-app', 'ts-base']);
    assert.deepEqual(
      (result?.result.steps?.[3]?.data?.files as Array<{ path: string }> | undefined)?.map(
        (file) => file.path,
      ).sort(),
      ['package.json', 'tsconfig.json'],
    );
    assert.deepEqual(
      (result?.result.steps?.[4]?.data?.files as Array<{ path: string }> | undefined)?.map(
        (file) => file.path,
      ),
      ['package.json', 'tsconfig.json'],
    );
    assert.deepEqual(result?.result.steps?.[5]?.data?.writtenFiles, [
      'package.json',
      'tsconfig.json',
    ]);
    assert.equal(result?.result.steps?.[6]?.data?.manager, 'pnpm');
    assert.deepEqual(result?.result.steps?.[6]?.data?.dependencies, ['react']);
    assert.deepEqual(result?.result.steps?.[6]?.data?.devDependencies, ['typescript']);
    assert.deepEqual(result?.result.steps?.[6]?.data?.executedCommands, [
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', 'react'],
        exitCode: 0,
      },
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', '--save-dev', 'typescript'],
        exitCode: 0,
      },
    ]);
    assert.equal(result?.result.steps?.[7]?.data?.executed, true);
    assert.equal(result?.result.steps?.[7]?.data?.status, 'initialized');
    assert.deepEqual(
      (result?.result.steps?.[8]?.data?.actions as Array<{ name: string }> | undefined)?.map(
        (action) => action.name,
      ),
      ['install-if-enabled', 'git-init-if-enabled', 'summary'],
    );
    assert.equal(result?.result.steps?.[8]?.data?.source, 'scaffold.postActions');
    assert.equal(
      (result?.result.steps?.[8]?.data?.actions as Array<{ executed?: boolean; skipped?: boolean }> | undefined)?.[0]
        ?.executed,
      false,
    );
    assert.equal(
      (result?.result.steps?.[8]?.data?.actions as Array<{ status?: string }> | undefined)?.[1]?.status,
      'already_initialized',
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /writtenFiles: package\.json, tsconfig\.json/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /files\.created: package\.json/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /files\.merged: tsconfig\.json\(deep_merge\)/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /git: already_initialized/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /guards: node-ready/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /mergeRules: tsconfig-merge/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /postActions: install-if-enabled\[skipped:step execution disabled\], git-init-if-enabled\[already_initialized\], summary\[planned\]/,
    );
    assert.match(
      String((result?.result.steps?.[9]?.data?.lines as string[] | undefined)?.join('\n') ?? ''),
      /nextSteps: pnpm install, pnpm run dev/,
    );
    assert.deepEqual(
      (result?.result.steps?.[9]?.data?.mergedFiles as
        | Array<{ path: string; change: string; strategy: string; source: string }>
        | undefined),
      [
        {
          path: 'package.json',
          change: 'create',
          strategy: 'package_json',
          source: 'rendered',
        },
        {
          path: 'tsconfig.json',
          change: 'merge',
          strategy: 'deep_merge',
          source: 'merged',
        },
      ],
    );
    assert.deepEqual(
      (result?.result.steps?.[9]?.data?.postActions as
        | Array<{ name: string; type: string; status?: string; executed?: boolean; skipped?: boolean }>
        | undefined)
        ?.map((action) => ({
          name: action.name,
          type: action.type,
          status: action.status,
          executed: action.executed,
          skipped: action.skipped,
        })),
      [
        {
          name: 'install-if-enabled',
          type: 'install_dependencies',
          status: undefined,
          executed: false,
          skipped: true,
        },
        {
          name: 'git-init-if-enabled',
          type: 'git_init',
          status: 'already_initialized',
          executed: false,
          skipped: true,
        },
        {
          name: 'summary',
          type: 'print_summary',
          status: undefined,
          executed: undefined,
          skipped: undefined,
        },
      ],
    );
    assert.deepEqual(result?.result.steps?.[9]?.data?.nextSteps, ['pnpm install', 'pnpm run dev']);
    assert.deepEqual(result?.result.summary?.templateLayers, ['base-app', 'ts-base']);
    assert.deepEqual(result?.result.summary?.dependencies, ['react']);
    assert.deepEqual(result?.result.summary?.devDependencies, ['typescript']);
    assert.equal(result?.result.summary?.scripts?.dev, 'vite');
    assert.equal(result?.result.summary?.packageManager, 'pnpm');
    assert.deepEqual(result?.result.summary?.nextSteps, ['pnpm install', 'pnpm run dev']);

    const writtenPackageJson = JSON.parse(await readFile(`${cwd}/package.json`, 'utf8')) as {
      name?: string;
      packageManager?: string;
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
      scripts?: Record<string, string>;
    };
    const writtenTsConfig = await readFile(`${cwd}/tsconfig.json`, 'utf8');

    assert.equal(writtenPackageJson.name, 'workflow-demo');
    assert.equal(writtenPackageJson.packageManager, 'pnpm');
    assert.equal(writtenPackageJson.dependencies?.react, 'latest');
    assert.equal(writtenPackageJson.devDependencies?.typescript, 'latest');
    assert.equal(writtenPackageJson.scripts?.dev, 'vite');
    assert.match(writtenTsConfig, /compilerOptions/);
    assert.match(writtenTsConfig, /"strict": true/);
    assert.deepEqual(
      execCalls.filter((call) => call.program === 'pnpm'),
      [
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', 'react'],
        cwd,
      },
      {
        program: 'pnpm',
        args: ['install', '--strict-peer-dependencies=false', '--save-dev', 'typescript'],
        cwd,
      },
      ],
    );
    assert.equal(gitInitialized, true);
    assert.ok(logMessages.some((message) => message.includes('runtime: development')));
    assert.ok(logMessages.some((message) => message.includes('writtenFiles: package.json, tsconfig.json')));
  });

  test('command.invokeDynamic supports plugin handlers declared in schema plugins', async (t) => {
    const cwd = await createDynamicCommandProject({
      pluginContent: `export default {
        name: 'command-plugin',
        methods: ['commands.run'],
        async handle(method, params) {
          if (method !== 'commands.run') return null;
          return {
            result: {
              ok: true,
              path: params.path,
              optionValue: params.argv?.options?.name ?? null,
              exitCode: 0
            },
            events: []
          };
        }
      };`,
      manifestContent: `export default {
        plugins: [
          { package: './lania.plugin.js', methods: ['commands.run'] }
        ],
        commands: [
          {
            name: 'plugin-run',
            handler: { plugin: './lania.plugin.js', method: 'commands.run' }
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    await writeProjectFile(
      cwd,
      'package.json',
      JSON.stringify(
        {
          name: 'existing-app',
          private: true,
          scripts: {
            lint: 'eslint .'
          }
        },
        null,
        2,
      ),
    );

    const resolved = await handleExchange({
      id: 'req-schema-plugin-handler-1',
      method: 'commands.resolveDynamic',
      params: { cwd },
    });
    const resolveResult = resolved.response.result as
      | { handlers: Array<{ handlerId: string; target: { kind: string; path?: string[] } }> }
      | undefined;
    const handler = resolveResult?.handlers.find(
      (entry) =>
        entry.target.kind === 'manifest_command' &&
        entry.target.path?.join(' ') === 'plugin-run',
    );
    assert.ok(handler, 'expected schema plugin handler');

    const invocation = await handleExchange({
      id: 'req-schema-plugin-handler-2',
      method: 'command.invokeDynamic',
      params: {
        cwd,
        handlerId: handler.handlerId,
        argv: { args: {}, options: { name: 'demo' } },
        target: handler.target,
      },
    });

    const result = invocation.response.result as
      | {
          exitCode: number;
          result: {
            ok: boolean;
            path: string[];
            optionValue: string;
          };
        }
      | undefined;

    assert.equal(invocation.response.error, undefined);
    assert.equal(result?.exitCode, 0);
    assert.equal(result?.result.ok, true);
    assert.deepEqual(result?.result.path, ['plugin-run']);
    assert.equal(result?.result.optionValue, 'demo');
  });
}
