import test from 'node:test';
import assert from 'node:assert/strict';

import { definePlugin, defineProduct, defineTemplate, defineWorkflow } from './index.js';

test('defineProduct returns the same product definition', () => {
  const product = defineProduct({
    name: '@acme/cli',
    binaryName: 'acme',
    versionStrategy: 'package_json',
  });

  assert.equal(product.name, '@acme/cli');
  assert.equal(product.binaryName, 'acme');
  assert.equal(product.versionStrategy, 'package_json');
});

test('defineWorkflow returns named workflow definitions', async () => {
  const workflow = defineWorkflow('createReactApp', async () => ({ ok: true }));

  assert.equal(workflow.name, 'createReactApp');
  assert.equal(typeof workflow.handler, 'function');
  if (typeof workflow.handler !== 'function') {
    throw new Error('expected named workflow handler to be a function');
  }
  assert.deepEqual(await workflow.handler({} as never), { ok: true });
});

test('defineWorkflow preserves declarative transaction options for author DSL', () => {
  const workflow = defineWorkflow({
    steps: [
      {
        name: 'installDependencies',
        options: {
          execute: true,
          transaction: {
            label: 'deps-install',
            target: 'workspace dependencies',
            rollback: {
              kind: 'compensation',
              reason: 'custom compensation',
              compensation: {
                commands: [{ program: 'pnpm', args: ['remove', 'react'] }],
                notes: ['Review lockfile before compensation.'],
              },
            },
          },
        },
      },
    ],
  });

  assert.equal(typeof workflow, 'object');
  if (typeof workflow !== 'object' || !('steps' in workflow)) {
    throw new Error('expected declarative workflow definition');
  }
  const step = workflow.steps[0];
  if (!step || typeof step === 'string') {
    throw new Error('expected workflow step object');
  }
  assert.equal(step.options?.transaction?.label, 'deps-install');
  assert.equal(step.options?.transaction?.target, 'workspace dependencies');
  assert.deepEqual(step.options?.transaction?.rollback, {
    kind: 'compensation',
    reason: 'custom compensation',
    compensation: {
      commands: [{ program: 'pnpm', args: ['remove', 'react'] }],
      notes: ['Review lockfile before compensation.'],
    },
  });
});

test('defineTemplate returns template definitions', () => {
  const template = defineTemplate({
    id: 'react-vite',
    title: 'React + Vite',
    variables: [{ key: 'projectName', type: 'string', required: true }],
  });

  assert.equal(template.id, 'react-vite');
  assert.equal(template.title, 'React + Vite');
  assert.equal(template.variables?.[0]?.key, 'projectName');
});

test('definePlugin returns plugin definitions', () => {
  const plugin = definePlugin({
    name: 'acme-plugin',
    package: '@acme/lania-plugin',
    methods: ['commands.schemaToCommands'],
  });

  assert.equal(plugin.name, 'acme-plugin');
  assert.equal(plugin.package, '@acme/lania-plugin');
  assert.equal(plugin.methods?.[0], 'commands.schemaToCommands');
});
