import type {
  DynamicCommandContext,
  WorkflowStepDefinition,
} from '../types.js';
import type {
  ScaffoldDependencyPlanResult,
  ScaffoldRenderResult,
} from '../../../core/schema-tools.js';
import { mergeScaffoldFiles } from '../merge-engine.js';

import type { WorkflowExecutionState } from './state.js';
import {
  readContext,
  readIncludeTemplateDependencies,
  readLayers,
  readManager,
  readOptions,
} from './options.js';

// Render / dependency planning / merge preparation are memoized because several steps reuse them.

export async function ensureRendered(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<ScaffoldRenderResult> {
  if (!state.rendered) {
    state.rendered = await ctx.tools.scaffold.renderTemplateLayers({
      layers: readLayers(step.options),
      context: readContext(step.options),
      options: readOptions(step.options),
    });
  }
  return state.rendered;
}

export async function ensureDependencyPlan(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<ScaffoldDependencyPlanResult> {
  if (!state.dependencyPlan) {
    state.dependencyPlan = await ctx.tools.scaffold.dependencyPlan({
      layers: readLayers(step.options),
      manager: readManager(step.options),
      includeTemplateDependencies: readIncludeTemplateDependencies(step.options),
      context: readContext(step.options),
      options: readOptions(step.options),
    });
  }
  return state.dependencyPlan;
}

export async function ensureMergedFiles(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Array<{ path: string; content: string }>> {
  if (state.mergedFiles) {
    return state.mergedFiles;
  }

  const rendered = await ensureRendered(step, ctx, state);
  const dependencyPlan = await ensureDependencyPlan(step, ctx, state);
  const merged = await mergeScaffoldFiles({
    renderedFiles: rendered.files,
    dependencyPlan,
    mergeRules: ctx.scaffold.mergeRules,
    readExistingFile: async (filePath) => await readExistingTextFile(filePath, ctx),
    readExistingPackageJson: async () => await loadExistingPackageJson(ctx),
  });
  state.mergedFileResults = merged.files.map((file) => ({ ...file }));
  state.mergedFiles = merged.files.map((file) => ({
    path: file.path,
    content: file.content,
  }));
  return state.mergedFiles;
}

async function readExistingTextFile(
  filePath: string,
  ctx: DynamicCommandContext,
): Promise<string | null> {
  if (!(await ctx.tools.fs.exists(filePath))) {
    return null;
  }
  return await ctx.tools.fs.read(filePath);
}

async function loadExistingPackageJson(
  ctx: DynamicCommandContext,
): Promise<Record<string, unknown> | null> {
  return await ctx.tools.workspace.packageJson();
}

