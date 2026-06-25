import type {
  DynamicCommandContext,
} from '../types.js';
import type { ScaffoldDependencyPlanResult } from '../../../core/schema-tools.js';
import { createTransactionSummary } from '../transaction-engine.js';

import type { WorkflowExecutionState } from './state.js';
import { joinOrNone, uniqueStrings } from './shared.js';

// Summary helpers translate detailed workflow state into:
// - human log lines
// - host-facing compact summary
// - actionable "next steps"

export function createWorkflowSummary(
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Record<string, unknown> & { lines: string[] } {
  const manager = state.dependencyPlan?.manager ?? ctx.scaffold.packageManager ?? null;
  const writtenFiles = [...state.writtenFiles];
  const mergedFiles = state.mergedFileResults ?? [];
  const createdFiles = mergedFiles.filter((file) => file.change === 'create').map((file) => file.path);
  const mergedTargets = mergedFiles
    .filter((file) => file.change === 'merge')
    .map((file) => `${file.path}(${file.strategy})`);
  const replacedFiles = mergedFiles
    .filter((file) => file.change === 'replace')
    .map((file) => `${file.path}(${file.strategy})`);
  const dependencySummary = summarizePackageJsonPatch(state.dependencyPlan, 'dependencies');
  const devDependencySummary = summarizePackageJsonPatch(state.dependencyPlan, 'devDependencies');
  const scriptSummary = summarizeScripts(state.dependencyPlan);
  const postActionLabels = summarizePostActions(state.postActionResults);
  const nextSteps = createWorkflowNextSteps(ctx, state);
  const transactionSummary = createTransactionSummary(state.transaction);
  const hostSummary = createHostFacingSummary(ctx, state);
  const lines = [
    `runtime: ${ctx.runtime.mode}`,
    `workspaceRoot: ${ctx.runtime.workspaceRoot}`,
    `productRoot: ${ctx.runtime.productRoot}`,
    `templateLayers: ${joinOrNone(ctx.scaffold.templateLayers)}`,
    `writtenFiles: ${joinOrNone(writtenFiles)}`,
    `files.created: ${joinOrNone(createdFiles)}`,
    `files.merged: ${joinOrNone(mergedTargets)}`,
    `files.replaced: ${joinOrNone(replacedFiles)}`,
    `dependencies(${manager ?? 'none'}): ${joinOrNone(ctx.scaffold.dependencies)}`,
    `devDependencies(${manager ?? 'none'}): ${joinOrNone(ctx.scaffold.devDependencies)}`,
    `packageJson.dependencies: ${joinOrNone(dependencySummary)}`,
    `packageJson.devDependencies: ${joinOrNone(devDependencySummary)}`,
    `packageJson.scripts: ${joinOrNone(scriptSummary)}`,
    `guards: ${joinOrNone(ctx.scaffold.guards.map((guard) => guard.name))}`,
    `mergeRules: ${joinOrNone(ctx.scaffold.mergeRules.map((rule) => rule.name))}`,
    `git: ${state.gitStatus ?? 'not_requested'}`,
    `postActions: ${joinOrNone(postActionLabels)}`,
    `nextSteps: ${joinOrNone(nextSteps)}`,
    `transaction.applied: ${joinOrNone(
      transactionSummary.operations
        .filter((operation) => operation.status === 'applied')
        .map((operation) => `${operation.step}(${operation.target})`),
    )}`,
    `transaction.nonRevertible: ${joinOrNone(transactionSummary.nonRevertible)}`,
    `transaction.compensations: ${joinOrNone(transactionSummary.compensations)}`,
  ];

  if (state.installedCommands && state.installedCommands.length > 0) {
    lines.push(`installCommands: ${state.installedCommands.length}`);
  }
  if (transactionSummary.rolledBack) {
    lines.push(
      `transaction.rollback: ${joinOrNone(
        transactionSummary.operations
          .filter((operation) => operation.rollback === 'completed')
          .map((operation) => `${operation.step}(${operation.target})`),
      )}`,
    );
  }

  return {
    lines,
    runtimeMode: ctx.runtime.mode,
    workspaceRoot: ctx.runtime.workspaceRoot,
    productRoot: ctx.runtime.productRoot,
    templateLayers: [...ctx.scaffold.templateLayers],
    writtenFiles,
    dependencies: [...ctx.scaffold.dependencies],
    devDependencies: [...ctx.scaffold.devDependencies],
    packageManager: manager,
    mergedFiles: mergedFiles.map((file) => ({
      path: file.path,
      strategy: file.strategy,
      source: file.source,
      change: file.change,
    })),
    guards: ctx.scaffold.guards.map((guard) => ({ ...guard })),
    mergeRules: ctx.scaffold.mergeRules.map((rule) => ({ ...rule })),
    gitStatus: state.gitStatus ?? 'not_requested',
    postActions: state.postActionResults.map((action) => ({ ...action })),
    nextSteps,
    transaction: transactionSummary,
    host: hostSummary,
  };
}

export function createHostFacingSummary(
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): {
  runtime: {
    mode: 'development' | 'installed';
    workspaceRoot: string;
    productRoot: string;
  };
  files: {
    written: string[];
    created: string[];
    merged: string[];
    replaced: string[];
  };
  packageJson: {
    dependencies: string[];
    devDependencies: string[];
    scripts: string[];
  };
  postActions: string[];
  nextSteps: string[];
  transaction: {
    applied: string[];
    rolledBack: string[];
    nonRevertible: string[];
    compensations: string[];
    rollbackFailures: string[];
    rolledBackAny: boolean;
  };
} {
  const mergedFiles = state.mergedFileResults ?? [];
  const transactionSummary = createTransactionSummary(state.transaction);
  return {
    runtime: {
      mode: ctx.runtime.mode,
      workspaceRoot: ctx.runtime.workspaceRoot,
      productRoot: ctx.runtime.productRoot,
    },
    files: {
      written: [...state.writtenFiles],
      created: mergedFiles.filter((file) => file.change === 'create').map((file) => file.path),
      merged: mergedFiles
        .filter((file) => file.change === 'merge')
        .map((file) => `${file.path}(${file.strategy})`),
      replaced: mergedFiles
        .filter((file) => file.change === 'replace')
        .map((file) => `${file.path}(${file.strategy})`),
    },
    packageJson: {
      dependencies: summarizePackageJsonPatch(state.dependencyPlan, 'dependencies'),
      devDependencies: summarizePackageJsonPatch(state.dependencyPlan, 'devDependencies'),
      scripts: summarizeScripts(state.dependencyPlan),
    },
    postActions: summarizePostActions(state.postActionResults),
    nextSteps: createWorkflowNextSteps(ctx, state),
    transaction: {
      applied: transactionSummary.operations
        .filter((operation) => operation.status === 'applied')
        .map((operation) => `${operation.step}(${operation.target})`),
      rolledBack: transactionSummary.operations
        .filter((operation) => operation.rollback === 'completed')
        .map((operation) => `${operation.step}(${operation.target})`),
      nonRevertible: [...transactionSummary.nonRevertible],
      compensations: [...transactionSummary.compensations],
      rollbackFailures: [...transactionSummary.rollbackFailures],
      rolledBackAny: transactionSummary.rolledBack,
    },
  };
}

export function summarizePackageJsonPatch(
  dependencyPlan: ScaffoldDependencyPlanResult | undefined,
  field: 'dependencies' | 'devDependencies',
): string[] {
  const entries = Object.entries(dependencyPlan?.packageJsonPatch[field] ?? {});
  return entries.map(([name, version]) => `${name}@${version}`);
}

export function summarizeScripts(
  dependencyPlan: ScaffoldDependencyPlanResult | undefined,
): string[] {
  return Object.entries(dependencyPlan?.packageJsonPatch.scripts ?? {}).map(
    ([name, command]) => `${name}=${command}`,
  );
}

export function summarizePostActions(actions: Array<Record<string, unknown>>): string[] {
  return actions.map((action) => {
    const name = String(action.name ?? action.type ?? 'action');
    const executed = action.executed === true;
    const skipped = action.skipped === true;
    const status =
      typeof action.status === 'string'
        ? action.status
        : executed
          ? 'executed'
          : skipped
            ? `skipped:${String(action.reason ?? 'unknown')}`
            : 'planned';
    return `${name}[${status}]`;
  });
}

export function createWorkflowNextSteps(
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): string[] {
  const manager = state.dependencyPlan?.manager ?? ctx.scaffold.packageManager ?? 'npm';
  const nextSteps: string[] = [];
  const hasDependencies =
    (state.dependencyPlan?.dependencies.length ?? 0) > 0 ||
    (state.dependencyPlan?.devDependencies.length ?? 0) > 0;
  const installWasExecuted = (state.installDependenciesResult?.executed as boolean | undefined) === true;

  if (hasDependencies && !installWasExecuted) {
    nextSteps.push(`${manager} install`);
  }

  if (state.dependencyPlan?.scripts.dev) {
    nextSteps.push(`${manager} run dev`);
  } else if (state.dependencyPlan?.scripts.start) {
    nextSteps.push(`${manager} run start`);
  }

  if ((state.gitStatus ?? 'not_requested') === 'not_requested' && state.writtenFiles.length > 0) {
    nextSteps.push('git init');
  }

  return uniqueStrings(nextSteps);
}
