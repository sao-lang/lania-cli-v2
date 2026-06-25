import type {
  WorkflowStepName,
} from '../types.js';
import type {
  ScaffoldDependencyPlanResult,
  ScaffoldRenderResult,
} from '../../../core/schema-tools.js';
import type { MergeEngineFileResult } from '../merge-engine.js';
import type { TransactionState } from '../transaction-engine.js';

// Shared mutable state threaded through declarative workflow execution.
//
// This used to live inline inside `workflow-steps.ts`, which made step handlers tightly coupled
// to one giant file. Moving the shape here lets handlers coordinate without reintroducing cycles.

export interface WorkflowExecutionState {
  rendered?: ScaffoldRenderResult;
  dependencyPlan?: ScaffoldDependencyPlanResult;
  mergedFiles?: Array<{ path: string; content: string }>;
  mergedFileResults?: MergeEngineFileResult[];
  writtenFiles: string[];
  installedCommands?: Array<{ program: string; args: string[]; exitCode: number }>;
  gitStatus?: 'initialized' | 'already_initialized' | 'planned' | 'not_requested';
  preflight?: Array<Record<string, unknown>>;
  currentStep?: WorkflowStepName;
  transaction: TransactionState;
  preparedRollbackFiles: Set<string>;
  postActionResults: Array<Record<string, unknown>>;
  installDependenciesResult?: Record<string, unknown>;
  gitInitResult?: Record<string, unknown>;
}

