import { defineSchemas, defineWorkflow } from 'lania';

const scaffoldAppWorkflow = defineWorkflow('scaffoldApp', {
  steps: [
    'preflight',
    'resolvePreset',
    'resolveFeatures',
    'renderTemplates',
    'mergeFiles',
    'writeFiles',
    {
      name: 'installDependencies',
      options: {
        execute: true,
        transaction: {
          label: 'deps-install',
          target: 'workspace dependencies',
          rollback: {
            kind: 'compensation',
            reason: 'dependency rollback requires uninstall compensation commands',
            compensation: {
              notes: ['Review package.json and lockfile changes before running compensation commands.'],
            },
          },
        },
      },
    },
    {
      name: 'gitInit',
      options: {
        skipIfExists: true,
        transaction: {
          label: 'workspace-git-init',
          rollback: {
            kind: 'compensation',
            reason: 'git initialization rollback requires manual cleanup',
            compensation: {
              notes: ['Remove .git only if it was created by this workflow and no extra history was added.'],
            },
          },
        },
      },
    },
    'printSummary',
  ],
});

export default defineSchemas({
  commands: [
    {
      name: 'create-app',
      about: 'Create a sample application',
      workflow: 'scaffoldApp',
      preset: 'base-app',
      features: ['typescript'],
    },
  ],
  workflows: {
    scaffoldApp: scaffoldAppWorkflow,
  },
  presets: {
    'base-app': {
      templateLayers: ['base-app'],
      dependencyRecipes: ['app-core'],
      guards: ['empty-dir', 'node-ready'],
      postActions: ['print-summary'],
    },
  },
  features: {
    typescript: {
      templateLayers: ['feature-typescript'],
      mergeRules: ['tsconfig-merge'],
    },
  },
  dependencyRecipes: {
    'app-core': {
      dependencies: ['react'],
      devDependencies: ['typescript'],
      scripts: {
        dev: 'vite',
      },
      packageManager: 'pnpm',
    },
  },
  mergeRules: {
    'tsconfig-merge': {
      target: 'tsconfig.json',
      strategy: 'deep_merge',
    },
  },
  guards: {
    'empty-dir': {
      type: 'directory_empty',
    },
    'node-ready': {
      type: 'node_version',
      range: '>=20',
    },
  },
  postActions: {
    'print-summary': {
      type: 'print_summary',
    },
  },
});
