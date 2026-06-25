import assert from 'node:assert/strict';
import test from 'node:test';

import {
  createTransactionState,
  createTransactionSummary,
  registerTransactionOperation,
  rollbackTransaction,
} from './transaction-engine.js';

test('transaction engine tracks hook, compensation, unsupported, and none rollback registrations', async () => {
  const state = createTransactionState();
  let rolledBack = false;

  registerTransactionOperation(state, {
    step: 'writeFiles',
    target: 'package.json',
    status: 'applied',
    rollback: {
      kind: 'hook',
      run: async () => {
        rolledBack = true;
      },
    },
  });
  registerTransactionOperation(state, {
    step: 'installDependencies',
    target: 'pnpm',
    status: 'applied',
    rollback: {
      kind: 'compensation',
      reason: 'dependency rollback requires uninstall compensation commands',
      plan: {
        commands: [{ program: 'pnpm', args: ['remove', 'react'] }],
        notes: ['Review lockfile changes before compensation.'],
      },
    },
  });
  registerTransactionOperation(state, {
    step: 'gitInit',
    target: 'git repository',
    status: 'applied',
    rollback: {
      kind: 'unsupported',
      reason: 'git init rollback is not implemented yet',
    },
  });
  registerTransactionOperation(state, {
    step: 'printSummary',
    target: 'summary output',
    status: 'planned',
    rollback: {
      kind: 'none',
      reason: 'step execution disabled',
    },
  });

  await rollbackTransaction(state);
  const summary = createTransactionSummary(state);

  assert.equal(rolledBack, true);
  assert.equal(summary.rolledBack, true);
  assert.deepEqual(summary.nonRevertible, ['installDependencies(pnpm)', 'gitInit(git repository)']);
  assert.deepEqual(summary.compensations, [
    'installDependencies(pnpm): pnpm remove react | Review lockfile changes before compensation.',
  ]);
  assert.deepEqual(
    summary.operations.map((operation) => ({
      step: operation.step,
      target: operation.target,
      status: operation.status,
      rollback: operation.rollback,
    })),
    [
      {
        step: 'writeFiles',
        target: 'package.json',
        status: 'rolled_back',
        rollback: 'completed',
      },
      {
        step: 'installDependencies',
        target: 'pnpm',
        status: 'applied',
        rollback: 'compensation_available',
      },
      {
        step: 'gitInit',
        target: 'git repository',
        status: 'applied',
        rollback: 'not_supported',
      },
      {
        step: 'printSummary',
        target: 'summary output',
        status: 'planned',
        rollback: 'not_needed',
      },
    ],
  );
});
