import { createTransactionSummary } from '../transaction-engine.js';
import type { WorkflowExecutionState } from './state.js';

// Wraps low-level errors with step/rollback context so the host sees actionable messages.

export function createWorkflowExecutionError(
  error: unknown,
  state: WorkflowExecutionState,
): Error {
  const baseMessage = error instanceof Error ? error.message : String(error);
  const rollbackMessage = state.transaction.rolledBack
    ? state.transaction.rollbackFailures.length > 0
      ? `rollback attempted with failures: ${state.transaction.rollbackFailures.join('; ')}`
      : 'rollback completed for file writes'
    : 'rollback not needed';
  const nonRevertible = createTransactionSummary(state.transaction).nonRevertible;
  const wrapped = new Error(
    `workflow step \`${state.currentStep ?? 'unknown'}\` failed: ${baseMessage} (${rollbackMessage}${
      nonRevertible.length > 0
        ? `; non-revertible operations executed: ${nonRevertible.join(', ')}`
        : ''
    })`,
  );
  (wrapped as Error & { cause?: unknown }).cause = error;
  return wrapped;
}

