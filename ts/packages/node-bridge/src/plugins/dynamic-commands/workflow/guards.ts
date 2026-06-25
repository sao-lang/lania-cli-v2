import type {
  DynamicCommandContext,
  DeclarativeWorkflowDefinition,
} from '../types.js';
import type { ScaffoldGuardPlan } from '../types.js';
import {
  detectCommandOnPath,
  detectWorkspaceKind,
  evaluateNodeVersionRange,
  formatGuardFailureMessage,
} from '../guard-engine.js';

import { assertNever, joinOrNone } from './shared.js';
import { readGuardNames } from './options.js';
import type { WorkflowExecutionState } from './state.js';

// Guard execution powers the `preflight` step and is intentionally isolated from mutation-heavy
// steps so failures happen before file writes / installs.

export async function executePreflight(
  step: { options?: Record<string, unknown> },
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
  workflow: DeclarativeWorkflowDefinition,
): Promise<Record<string, unknown>> {
  const guardNames = readGuardNames(step.options);
  const guards = resolveGuards(ctx.scaffold.guards, workflow.guards, guardNames);
  const results = await Promise.all(guards.map((guard) => runGuard(guard, ctx)));
  state.preflight = results;
  const failed = results.filter((result) => result.ok !== true);
  if (failed.length > 0) {
    throw new Error(
      `preflight failed: ${failed.map((result) => formatGuardFailureMessage(result)).join('; ')}`,
    );
  }
  return {
    guards: results,
  };
}

function resolveGuards(
  planned: ScaffoldGuardPlan[],
  workflowGuards?: string[],
  stepGuards?: string[],
): ScaffoldGuardPlan[] {
  const lookup = new Map(planned.map((guard) => [guard.name, guard]));
  const requestedNames = stepGuards ?? workflowGuards;
  if (!requestedNames || requestedNames.length === 0) {
    return planned;
  }
  return requestedNames
    .map((name) => lookup.get(name))
    .filter((guard): guard is ScaffoldGuardPlan => Boolean(guard));
}

async function runGuard(
  guard: ScaffoldGuardPlan,
  ctx: DynamicCommandContext,
): Promise<Record<string, unknown>> {
  switch (guard.type) {
    case 'directory_empty': {
      const entries = await ctx.tools.fs.readdir('.');
      const visibleEntries = entries.filter((entry) => entry !== '.git');
      return {
        name: guard.name,
        type: guard.type,
        ok: visibleEntries.length === 0,
        entries: visibleEntries,
      };
    }
    case 'node_version': {
      const result = await ctx.tools.exec.runChecked({
        program: 'node',
        args: ['--version'],
        cwd: ctx.cwd,
      });
      const evaluation = evaluateNodeVersionRange(result.stdout.trim(), guard.range);
      return {
        name: guard.name,
        type: guard.type,
        ok: result.exitCode === 0 && evaluation.ok,
        range: evaluation.normalizedRange,
        version: evaluation.normalizedVersion ?? result.stdout.trim(),
        message: evaluation.reason,
      };
    }
    case 'command_exists': {
      const result = await detectCommandOnPath(guard.command, ctx.tools.env.get('PATH'));
      return {
        name: guard.name,
        type: guard.type,
        ok: result.ok,
        command: guard.command,
        resolvedPath: result.resolvedPath,
        message: result.reason,
      };
    }
    case 'workspace_kind': {
      const packageJson = await ctx.tools.workspace.packageJson(ctx.cwd);
      const detected = await detectWorkspaceKind({
        packageJson,
        hasFile: async (filePath) => await ctx.tools.workspace.hasFile(filePath),
      });
      return {
        name: guard.name,
        type: guard.type,
        ok: detected.kind === guard.value,
        expected: guard.value,
        actual: detected.kind,
        indicators: detected.indicators,
        message:
          detected.kind === guard.value
            ? undefined
            : `expected ${guard.value}, detected ${detected.kind} (${joinOrNone(detected.indicators)})`,
      };
    }
    default:
      return assertNever(guard);
  }
}

