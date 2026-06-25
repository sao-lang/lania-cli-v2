import assert from 'node:assert/strict';
import test from 'node:test';

import type { SchemaTools } from '../../core/schema-tools.js';
import type { DynamicCommandContext, ScaffoldPlan } from './types.js';
import { executeDeclarativeWorkflow } from './workflow-steps.js';

test('executeDeclarativeWorkflow rolls back file writes when writeFiles fails', async () => {
  const workspaceFiles = new Map<string, string>([['a.txt', 'before-a']]);

  const ctx = createContext({
    renderFiles: [
      { path: 'a.txt', content: 'after-a' },
      { path: 'b.txt', content: 'after-b' },
    ],
    fileState: workspaceFiles,
    failOnWritePath: 'b.txt',
  });

  await assert.rejects(
    async () =>
      await executeDeclarativeWorkflow(
        {
          steps: ['mergeFiles', 'writeFiles'],
        },
        ctx,
      ),
    /rollback completed for file writes/,
  );

  assert.equal(workspaceFiles.get('a.txt'), 'before-a');
  assert.equal(workspaceFiles.has('b.txt'), false);
});

test('executeDeclarativeWorkflow reports detailed preflight failures', async () => {
  const ctx = createContext({
    renderFiles: [],
    fileState: new Map(),
    packageJson: {
      name: 'demo-app',
    },
    nodeVersion: 'v16.0.0\n',
    pathEnv: '/missing-bin',
    scaffold: {
      guards: [
        { name: 'node-ready', type: 'node_version', range: '>=18' },
        { name: 'git-ready', type: 'command_exists', command: 'git' },
        { name: 'monorepo-only', type: 'workspace_kind', value: 'monorepo' },
      ],
    },
  });

  await assert.rejects(
    async () =>
      await executeDeclarativeWorkflow(
        {
          steps: ['preflight'],
        },
        ctx,
      ),
    /node-ready: expected >=18, received 16\.0\.0; git-ready: command `git` was not found in PATH; monorepo-only: expected monorepo, detected single \(none\)/,
  );
});

test('executeDeclarativeWorkflow reports non-revertible transaction operations', async () => {
  const workspaceFiles = new Map<string, string>();
  const ctx = createContext({
    renderFiles: [{ path: 'a.txt', content: 'after-a' }],
    fileState: workspaceFiles,
    failOnWritePath: 'a.txt',
    dependencyPlan: {
      manager: 'pnpm',
      templates: [],
      dependencies: ['react'],
      devDependencies: [],
      scripts: {},
      packageJsonPatch: {
        dependencies: { react: 'latest' },
        devDependencies: {},
        scripts: {},
      },
      installCommands: [{ program: 'pnpm', args: ['install', 'react'] }],
    },
  });

  await assert.rejects(
    async () =>
      await executeDeclarativeWorkflow(
        {
          steps: ['installDependencies', 'mergeFiles', 'writeFiles'],
        },
        ctx,
      ),
    /non-revertible operations executed: installDependencies\(pnpm\)/,
  );
});

test('executeDeclarativeWorkflow exposes compensation plans in summary', async () => {
  const workspaceFiles = new Map<string, string>();
  const ctx = createContext({
    renderFiles: [{ path: 'package.json', content: '{\n  "name": "demo"\n}\n' }],
    fileState: workspaceFiles,
    dependencyPlan: {
      manager: 'pnpm',
      templates: [],
      dependencies: ['react'],
      devDependencies: ['typescript'],
      scripts: { dev: 'vite' },
      packageJsonPatch: {
        dependencies: { react: 'latest' },
        devDependencies: { typescript: 'latest' },
        scripts: { dev: 'vite' },
      },
      installCommands: [
        { program: 'pnpm', args: ['install', 'react'] },
        { program: 'pnpm', args: ['install', '--save-dev', 'typescript'] },
      ],
    },
  });

  const result = await executeDeclarativeWorkflow(
    {
      steps: ['installDependencies', 'mergeFiles', 'writeFiles', 'printSummary'],
    },
    ctx,
  );

  assert.deepEqual(result.summary.transaction?.operations[0], {
    step: 'installDependencies',
    target: 'pnpm',
    status: 'applied',
    rollback: 'compensation_available',
    reason: 'dependency rollback requires uninstall compensation commands',
    compensation: {
      commands: [{ program: 'pnpm', args: ['remove', 'react', 'typescript'] }],
      notes: ['Review package.json and lockfile changes before running compensation commands.'],
    },
  });
  assert.deepEqual(result.summary.transaction?.compensations, [
    'installDependencies(pnpm): pnpm remove react typescript | Review package.json and lockfile changes before running compensation commands.',
  ]);
  assert.deepEqual(result.summary.host, {
    runtime: {
      mode: 'development',
      workspaceRoot: '/workspace',
      productRoot: '/product',
    },
    files: {
      written: ['package.json'],
      created: ['package.json'],
      merged: [],
      replaced: [],
    },
    packageJson: {
      dependencies: ['react@latest'],
      devDependencies: ['typescript@latest'],
      scripts: ['dev=vite'],
    },
    postActions: [],
    nextSteps: ['pnpm run dev', 'git init'],
    transaction: {
      applied: ['installDependencies(pnpm)', 'writeFiles(package.json)'],
      rolledBack: [],
      nonRevertible: ['installDependencies(pnpm)'],
      compensations: [
        'installDependencies(pnpm): pnpm remove react typescript | Review package.json and lockfile changes before running compensation commands.',
      ],
      rollbackFailures: [],
      rolledBackAny: false,
    },
  });
});

test('executeDeclarativeWorkflow honors author-defined transaction overrides', async () => {
  const ctx = createContext({
    renderFiles: [],
    fileState: new Map(),
    dependencyPlan: {
      manager: 'pnpm',
      templates: [],
      dependencies: ['react'],
      devDependencies: [],
      scripts: {},
      packageJsonPatch: {
        dependencies: { react: 'latest' },
        devDependencies: {},
        scripts: {},
      },
      installCommands: [{ program: 'pnpm', args: ['install', 'react'] }],
    },
  });

  const result = await executeDeclarativeWorkflow(
    {
      steps: [
        {
          name: 'installDependencies',
          options: {
            transaction: {
              label: 'deps-install',
              target: 'workspace package set',
              rollback: {
                kind: 'compensation',
                reason: 'author supplied compensation plan',
                compensation: {
                  notes: ['Run verification before uninstalling dependencies.'],
                },
              },
            },
          },
        },
      ],
    },
    ctx,
  );

  assert.deepEqual(result.summary.transaction?.operations[0], {
    step: 'deps-install',
    target: 'workspace package set',
    status: 'applied',
    rollback: 'compensation_available',
    reason: 'author supplied compensation plan',
    compensation: {
      commands: [{ program: 'pnpm', args: ['remove', 'react'] }],
      notes: ['Run verification before uninstalling dependencies.'],
    },
  });
});

function createContext(params: {
  renderFiles: Array<{ path: string; content: string }>;
  fileState: Map<string, string>;
  failOnWritePath?: string;
  packageJson?: Record<string, unknown> | null;
  nodeVersion?: string;
  pathEnv?: string;
  scaffold?: Partial<ScaffoldPlan>;
  dependencyPlan?: {
    manager: string;
    templates: string[];
    dependencies: string[];
    devDependencies: string[];
    scripts: Record<string, string>;
    packageJsonPatch: {
      dependencies: Record<string, string>;
      devDependencies: Record<string, string>;
      scripts: Record<string, string>;
    };
    installCommands: Array<{ program: string; args: string[] }>;
  };
}): DynamicCommandContext {
  const scaffold: ScaffoldPlan = {
    preset: null,
    features: [],
    templateLayers: [],
    dependencyRecipes: [],
    dependencies: [],
    devDependencies: [],
    scripts: {},
    packageManager: 'pnpm',
    resolvers: {},
    mergeRules: [],
    guards: [],
    postActions: [],
    ...(params.scaffold ?? {}),
  };

  const tools = {
    scaffold: {
      currentPlan: () => scaffold,
      renderTemplateLayers: async () => ({
        templates: [],
        files: params.renderFiles,
        collisions: [],
      }),
      dependencyPlan: async () => ({
        manager: params.dependencyPlan?.manager ?? 'pnpm',
        templates: params.dependencyPlan?.templates ?? [],
        dependencies: params.dependencyPlan?.dependencies ?? [],
        devDependencies: params.dependencyPlan?.devDependencies ?? [],
        scripts: params.dependencyPlan?.scripts ?? {},
        packageJsonPatch: params.dependencyPlan?.packageJsonPatch ?? {
          dependencies: {},
          devDependencies: {},
          scripts: {},
        },
        installCommands: params.dependencyPlan?.installCommands ?? [],
      }),
    },
    fs: {
      exists: async (filePath: string) => params.fileState.has(filePath),
      read: async (filePath: string) => {
        const content = params.fileState.get(filePath);
        if (typeof content !== 'string') {
          throw new Error(`missing file: ${filePath}`);
        }
        return content;
      },
      write: async (filePath: string, content: string) => {
        if (filePath === params.failOnWritePath) {
          throw new Error(`write failure for ${filePath}`);
        }
        params.fileState.set(filePath, content);
      },
      remove: async (filePath: string) => {
        const removed = params.fileState.delete(filePath);
        return { removed };
      },
    },
    workspace: {
      packageJson: async () => params.packageJson ?? null,
      hasFile: async (filePath: string) => params.fileState.has(filePath),
    },
    env: {
      get: (name: string) => (name === 'PATH' ? params.pathEnv ?? '' : undefined),
    },
    exec: {
      runChecked: async () => ({
        exitCode: 0,
        stdout: params.nodeVersion ?? 'v20.0.0\n',
        stderr: '',
      }),
    },
    pm: {
      command: {
        remove: async (packages: string[], options?: { manager?: string }) => ({
          program: options?.manager ?? 'npm',
          args: ['remove', ...packages],
        }),
      },
    },
  } as unknown as SchemaTools;

  return {
    cwd: '/workspace',
    mount: 'create',
    path: ['demo'],
    argv: { args: {}, options: {} },
    traceId: null,
    tools,
    scaffold,
    product: {
      name: '@acme/cli',
      binaryName: 'acme',
      displayName: 'Acme CLI',
      version: '0.0.0',
      productRoot: '/product',
      schemaRoot: '/product',
      templatesDir: '/product/templates',
    },
    runtime: {
      mode: 'development',
      traceId: null,
      invocationCwd: '/workspace',
      workspaceRoot: '/workspace',
      productRoot: '/product',
      schemaRoot: '/product',
    },
  };
}
