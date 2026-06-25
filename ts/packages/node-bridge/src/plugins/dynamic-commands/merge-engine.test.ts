import assert from 'node:assert/strict';
import test from 'node:test';

import type { ScaffoldDependencyPlanResult } from '../../core/schema-tools.js';
import { deepMergeRecords, mergeScaffoldFiles } from './merge-engine.js';
import type { ScaffoldMergeRulePlan } from './types.js';

test('mergeScaffoldFiles merges package.json without workflow special casing', async () => {
  const existingFiles = new Map<string, string>([
    [
      'package.json',
      JSON.stringify(
        {
          name: 'existing-app',
          private: true,
          scripts: {
            lint: 'eslint .',
          },
          keywords: ['base'],
        },
        null,
        2,
      ),
    ],
  ]);

  const result = await mergeScaffoldFiles({
    renderedFiles: [
      {
        path: 'package.json',
        content: JSON.stringify(
          {
            name: 'rendered-app',
            keywords: ['base', 'scaffold'],
            scripts: {
              dev: 'vite',
            },
          },
          null,
          2,
        ),
      },
    ],
    dependencyPlan: createDependencyPlan({
      dependencies: { react: '^19.0.0' },
      devDependencies: { typescript: '^5.0.0' },
      scripts: { build: 'tsc -b' },
    }),
    mergeRules: [{ name: 'pkg', target: 'package.json', strategy: 'deep_merge' }],
    readExistingFile: async (filePath) => existingFiles.get(filePath) ?? null,
    readExistingPackageJson: async () => JSON.parse(existingFiles.get('package.json') ?? 'null'),
  });

  assert.equal(result.files.length, 1);
  const packageJson = JSON.parse(result.files[0]!.content) as {
    name?: string;
    private?: boolean;
    keywords?: string[];
    scripts?: Record<string, string>;
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    packageManager?: string;
  };

  assert.equal(packageJson.name, 'rendered-app');
  assert.equal(packageJson.private, true);
  assert.deepEqual(packageJson.keywords, ['base', 'scaffold']);
  assert.deepEqual(packageJson.scripts, {
    lint: 'eslint .',
    dev: 'vite',
    build: 'tsc -b',
  });
  assert.deepEqual(packageJson.dependencies, { react: '^19.0.0' });
  assert.deepEqual(packageJson.devDependencies, { typescript: '^5.0.0' });
  assert.equal(packageJson.packageManager, 'pnpm');
});

test('mergeScaffoldFiles deep merges structured files with array dedupe strategy', async () => {
  const existingFiles = new Map<string, string>([
    [
      'tsconfig.json',
      JSON.stringify(
        {
          compilerOptions: {
            lib: ['ES2022', 'DOM'],
            paths: {
              '@/*': ['./src/*'],
            },
          },
        },
        null,
        2,
      ),
    ],
  ]);

  const result = await mergeScaffoldFiles({
    renderedFiles: [
      {
        path: 'tsconfig.json',
        content: JSON.stringify(
          {
            compilerOptions: {
              lib: ['DOM', 'DOM.Iterable'],
              strict: true,
            },
          },
          null,
          2,
        ),
      },
    ],
    dependencyPlan: createDependencyPlan(),
    mergeRules: [{ name: 'tsconfig', target: 'tsconfig.json', strategy: 'dedupe_merge' }],
    readExistingFile: async (filePath) => existingFiles.get(filePath) ?? null,
  });

  const tsconfig = JSON.parse(result.files[0]!.content) as {
    compilerOptions?: {
      strict?: boolean;
      lib?: string[];
      paths?: Record<string, string[]>;
    };
  };

  assert.equal(tsconfig.compilerOptions?.strict, true);
  assert.deepEqual(tsconfig.compilerOptions?.lib, ['ES2022', 'DOM', 'DOM.Iterable']);
  assert.deepEqual(tsconfig.compilerOptions?.paths, {
    '@/*': ['./src/*'],
  });
});

test('mergeScaffoldFiles materializes package.json from dependency plan when template omits it', async () => {
  const result = await mergeScaffoldFiles({
    renderedFiles: [],
    dependencyPlan: createDependencyPlan({
      dependencies: { react: 'latest' },
      scripts: { dev: 'vite' },
    }),
    mergeRules: [],
    readExistingFile: async () => null,
  });

  assert.deepEqual(result.files.map((file) => file.path), ['package.json']);
  const packageJson = JSON.parse(result.files[0]!.content) as {
    dependencies?: Record<string, string>;
    scripts?: Record<string, string>;
    packageManager?: string;
  };
  assert.deepEqual(packageJson.dependencies, { react: 'latest' });
  assert.deepEqual(packageJson.scripts, { dev: 'vite' });
  assert.equal(packageJson.packageManager, 'pnpm');
});

test('deepMergeRecords appends arrays when requested', () => {
  const merged = deepMergeRecords(
    {
      tags: ['base'],
      nested: {
        values: [1],
      },
    },
    {
      tags: ['feature'],
      nested: {
        values: [2],
      },
    },
    { arrayStrategy: 'append' },
  );

  assert.deepEqual(merged, {
    tags: ['base', 'feature'],
    nested: {
      values: [1, 2],
    },
  });
});

function createDependencyPlan(
  patch?: Partial<ScaffoldDependencyPlanResult['packageJsonPatch']>,
): ScaffoldDependencyPlanResult {
  return {
    manager: 'pnpm',
    templates: [],
    dependencies: [],
    devDependencies: [],
    scripts: {},
    packageJsonPatch: {
      dependencies: patch?.dependencies ?? {},
      devDependencies: patch?.devDependencies ?? {},
      scripts: patch?.scripts ?? {},
    },
    installCommands: [],
  };
}

const _unusedTypeCheck: ScaffoldMergeRulePlan | undefined = undefined;
