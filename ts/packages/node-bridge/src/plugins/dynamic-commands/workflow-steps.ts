import type {
  DeclarativeWorkflowDefinition,
  DynamicCommandContext,
  WorkflowExecutionResult,
  WorkflowStepDefinition,
  WorkflowStepResult,
} from './types.js';
import {
  createTransactionState,
  createTransactionSummary,
  registerTransactionOperation,
  rollbackTransaction,
} from './transaction-engine.js';

import type { WorkflowExecutionState } from './workflow/state.js';
import { ensureMergedFiles, ensureRendered } from './workflow/render-merge.js';
import { executeInstallDependencies, executeGitInit, executePostActions, executePrintSummary } from './workflow/actions.js';
import { executePreflight } from './workflow/guards.js';
import { createWorkflowExecutionError } from './workflow/error.js';
import { createFileRollbackHook } from './workflow/rollback.js';
import { createHostFacingSummary, createWorkflowNextSteps } from './workflow/summary.js';
import { assertNever, uniqueStrings } from './workflow/shared.js';

// Declarative workflow executor.
//
// This file stays intentionally thin after the split:
// - orchestration / step dispatch lives here
// - heavy implementation details live under `./workflow/*`
// This keeps the public import path stable for `parse-spec.ts` and tests.

export async function executeDeclarativeWorkflow(
  workflow: DeclarativeWorkflowDefinition,
  ctx: DynamicCommandContext,
): Promise<WorkflowExecutionResult> {
  const steps = normalizeSteps(workflow.steps);
  const results: WorkflowStepResult[] = [];
  const state: WorkflowExecutionState = {
    writtenFiles: [],
    transaction: createTransactionState(),
    preparedRollbackFiles: new Set<string>(),
    postActionResults: [],
  };

  try {
    for (const step of steps) {
      state.currentStep = step.name;
      results.push(await executeStep(step, ctx, state, workflow));
    }
  } catch (error) {
    await rollbackTransaction(state.transaction);
    throw createWorkflowExecutionError(error, state);
  }

  return {
    steps: results,
    summary: {
      templateLayers: [...ctx.scaffold.templateLayers],
      dependencies: [...ctx.scaffold.dependencies],
      devDependencies: [...ctx.scaffold.devDependencies],
      scripts: { ...ctx.scaffold.scripts },
      packageManager: ctx.scaffold.packageManager,
      writtenFiles: [...state.writtenFiles],
      mergedFiles: state.mergedFileResults?.map((file) => ({
        path: file.path,
        strategy: file.strategy,
        source: file.source,
        change: file.change,
      })),
      postActions: state.postActionResults.map((action) => ({ ...action })),
      nextSteps: createWorkflowNextSteps(ctx, state),
      transaction: createTransactionSummary(state.transaction),
      host: createHostFacingSummary(ctx, state),
    },
  };
}

function normalizeSteps(
  steps: DeclarativeWorkflowDefinition['steps'],
): WorkflowStepDefinition[] {
  return steps.map((step) => (typeof step === 'string' ? { name: step } : step));
}

async function executeStep(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
  workflow: DeclarativeWorkflowDefinition,
): Promise<WorkflowStepResult> {
  switch (step.name) {
    case 'preflight':
      return {
        step: step.name,
        ok: true,
        data: await executePreflight(step, ctx, state, workflow),
      };
    case 'resolvePreset':
      return {
        step: step.name,
        ok: true,
        data: {
          preset: ctx.scaffold.preset,
        },
      };
    case 'resolveFeatures':
      return {
        step: step.name,
        ok: true,
        data: {
          features: [...ctx.scaffold.features],
        },
      };
    case 'renderTemplates': {
      const rendered = await ensureRendered(step, ctx, state);
      return {
        step: step.name,
        ok: true,
        data: {
          templates: rendered.templates,
          files: rendered.files,
          collisions: rendered.collisions,
        },
      };
    }
    case 'mergeFiles': {
      const mergedFiles = await ensureMergedFiles(step, ctx, state);
      return {
        step: step.name,
        ok: true,
        data: {
          files: mergedFiles,
        },
      };
    }
    case 'writeFiles': {
      const mergedFiles = await ensureMergedFiles(step, ctx, state);
      const writtenFiles: string[] = [];
      for (const file of mergedFiles) {
        // Each file write registers a rollback hook so any later failure can restore the workspace.
        const rollbackHook = await createFileRollbackHook(file.path, ctx, state);
        await ctx.tools.fs.write(file.path, file.content, { mkdirp: true });
        registerTransactionOperation(state.transaction, {
          step: 'writeFiles',
          target: file.path,
          status: 'applied',
          rollback: { kind: 'hook', run: rollbackHook },
        });
        writtenFiles.push(file.path);
      }
      state.writtenFiles = uniqueStrings([...state.writtenFiles, ...writtenFiles]);
      return {
        step: step.name,
        ok: true,
        data: {
          writtenFiles,
        },
      };
    }
    case 'installDependencies':
      return {
        step: step.name,
        ok: true,
        data: await executeInstallDependencies(step, ctx, state),
      };
    case 'gitInit':
      return {
        step: step.name,
        ok: true,
        data: await executeGitInit(step, ctx, state),
      };
    case 'postActions':
      return {
        step: step.name,
        ok: true,
        data: await executePostActions(step, ctx, state),
      };
    case 'printSummary':
      return {
        step: step.name,
        ok: true,
        data: await executePrintSummary(step, ctx, state),
      };
    default:
      return assertNever(step.name);
  }
}

