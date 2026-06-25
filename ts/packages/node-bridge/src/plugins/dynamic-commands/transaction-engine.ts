export type TransactionOperationStatus = 'applied' | 'skipped' | 'planned' | 'rolled_back';
export type TransactionRollbackStatus =
  | 'supported'
  | 'completed'
  | 'not_needed'
  | 'not_supported'
  | 'compensation_available'
  | 'failed';

export interface TransactionCompensationPlan {
  commands?: Array<{ program: string; args: string[] }>;
  notes?: string[];
}

export interface TransactionOperation {
  id: number;
  step: string;
  target: string;
  status: TransactionOperationStatus;
  rollback: TransactionRollbackStatus;
  reason?: string;
  compensation?: TransactionCompensationPlan;
}

export interface TransactionSummary {
  operations: Array<{
    step: string;
    target: string;
    status: TransactionOperationStatus;
    rollback: TransactionRollbackStatus;
    reason?: string;
      compensation?: TransactionCompensationPlan;
  }>;
  rolledBack: boolean;
  rollbackFailures: string[];
  nonRevertible: string[];
  compensations: string[];
}

export interface TransactionState {
  rollbackHooks: Array<{ operationId: number; run: () => Promise<void> }>;
  rolledBack: boolean;
  rollbackFailures: string[];
  operations: TransactionOperation[];
  nextOperationId: number;
}

export type TransactionRollbackRegistration =
  | { kind: 'hook'; run: () => Promise<void> }
  | { kind: 'unsupported'; reason: string }
  | { kind: 'none'; reason?: string }
  | { kind: 'compensation'; reason: string; plan: TransactionCompensationPlan };

export function mergeTransactionCompensationPlan(
  base: TransactionCompensationPlan | undefined,
  override: TransactionCompensationPlan | undefined,
): TransactionCompensationPlan | undefined {
  if (!base && !override) {
    return undefined;
  }
  const commands = override?.commands ?? base?.commands;
  const notes = override?.notes ?? base?.notes;
  return {
    ...(commands ? { commands: commands.map((command) => ({ program: command.program, args: [...command.args] })) } : {}),
    ...(notes ? { notes: [...notes] } : {}),
  };
}

export interface RegisterTransactionOperationInput {
  step: string;
  target: string;
  status: Exclude<TransactionOperationStatus, 'rolled_back'>;
  reason?: string;
  rollback: TransactionRollbackRegistration;
}

export function createTransactionState(): TransactionState {
  return {
    rollbackHooks: [],
    rolledBack: false,
    rollbackFailures: [],
    operations: [],
    nextOperationId: 1,
  };
}

export function registerTransactionOperation(
  state: TransactionState,
  operation: RegisterTransactionOperationInput,
): TransactionOperation {
  const entry: TransactionOperation = {
    id: state.nextOperationId,
    step: operation.step,
    target: operation.target,
    status: operation.status,
    rollback: mapRollbackRegistrationToStatus(operation.rollback),
    reason: operation.reason ?? readRollbackReason(operation.rollback),
    compensation: readCompensation(operation.rollback),
  };
  state.nextOperationId += 1;
  state.operations.push(entry);

  if (operation.rollback.kind === 'hook') {
    state.rollbackHooks.push({
      operationId: entry.id,
      run: operation.rollback.run,
    });
  }

  return entry;
}

export async function rollbackTransaction(state: TransactionState): Promise<void> {
  for (const hook of [...state.rollbackHooks].reverse()) {
    try {
      await hook.run();
      const operation = state.operations.find((entry) => entry.id === hook.operationId);
      if (operation) {
        operation.status = 'rolled_back';
        operation.rollback = 'completed';
      }
    } catch (error) {
      state.rollbackFailures.push(error instanceof Error ? error.message : String(error));
      const operation = state.operations.find((entry) => entry.id === hook.operationId);
      if (operation) {
        operation.rollback = 'failed';
        operation.reason = error instanceof Error ? error.message : String(error);
      }
    }
  }
  state.rolledBack = true;
}

export function createTransactionSummary(state: TransactionState): TransactionSummary {
  return {
    operations: state.operations.map((operation) => ({
      step: operation.step,
      target: operation.target,
      status: operation.status,
      rollback: operation.rollback,
      reason: operation.reason,
      compensation: operation.compensation,
    })),
    rolledBack: state.rolledBack,
    rollbackFailures: [...state.rollbackFailures],
    nonRevertible: state.operations
      .filter((operation) =>
        operation.status === 'applied' &&
        (operation.rollback === 'not_supported' || operation.rollback === 'compensation_available'),
      )
      .map((operation) => `${operation.step}(${operation.target})`),
    compensations: state.operations
      .filter(
        (operation) => operation.status === 'applied' && operation.rollback === 'compensation_available',
      )
      .map((operation) => formatCompensationLabel(operation)),
  };
}

function mapRollbackRegistrationToStatus(
  rollback: TransactionRollbackRegistration,
): TransactionRollbackStatus {
  switch (rollback.kind) {
    case 'hook':
      return 'supported';
    case 'unsupported':
      return 'not_supported';
    case 'none':
      return 'not_needed';
    case 'compensation':
      return 'compensation_available';
    default:
      return assertNever(rollback);
  }
}

function readRollbackReason(rollback: TransactionRollbackRegistration): string | undefined {
  switch (rollback.kind) {
    case 'unsupported':
      return rollback.reason;
    case 'none':
      return rollback.reason;
    case 'hook':
      return undefined;
    case 'compensation':
      return rollback.reason;
    default:
      return assertNever(rollback);
  }
}

function readCompensation(
  rollback: TransactionRollbackRegistration,
): TransactionCompensationPlan | undefined {
  return rollback.kind === 'compensation' ? rollback.plan : undefined;
}

function formatCompensationLabel(operation: TransactionOperation): string {
  const commandPreview =
    operation.compensation?.commands?.map((command) => `${command.program} ${command.args.join(' ')}`.trim()) ??
    [];
  const notePreview = operation.compensation?.notes ?? [];
  const detail = [...commandPreview, ...notePreview].join(' | ');
  return detail.length > 0 ? `${operation.step}(${operation.target}): ${detail}` : `${operation.step}(${operation.target})`;
}

function assertNever(value: never): never {
  throw new Error(`unsupported transaction registration: ${String(value)}`);
}
