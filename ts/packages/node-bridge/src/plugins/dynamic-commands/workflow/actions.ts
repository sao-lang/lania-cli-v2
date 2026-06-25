/**
 * workflow 阶段里所有“后置副作用步骤”的执行器集合。
 *
 * 前面的 render / merge 阶段主要决定要写什么文件；
 * 到这个模块时，关注点转成了“写完之后还要不要继续改工作区状态”，例如：
 * - 安装依赖
 * - 初始化 git / 首次提交
 * - 输出 workflow 摘要
 *
 * 它们被放在同一个模块里，是因为都需要：
 * - 读取 workflow step options 判定是否真的执行
 * - 回写 `WorkflowExecutionState`
 * - 向 transaction engine 注册补偿/回滚信息
 */
import type {
  DynamicCommandContext,
  WorkflowStepDefinition,
  ScaffoldPostActionPlan,
} from '../types.js';
import type { ScaffoldDependencyPlanResult } from '../../../core/schema-tools.js';
import { registerTransactionOperation } from '../transaction-engine.js';

import type { WorkflowExecutionState } from './state.js';
import { ensureDependencyPlan } from './render-merge.js';
import {
  readActionNames,
  readBooleanOption,
  readTransactionLabel,
  readTransactionTarget,
  shouldExecuteStep,
} from './options.js';
import { resolveTransactionRegistration } from './transaction.js';
import { createWorkflowSummary } from './summary.js';
import { uniqueStrings, assertNever } from './shared.js';

export async function executeInstallDependencies(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // installDependencies 的输入最终都来自 `ensureDependencyPlan`：
  // 这里不再重新推导依赖，只负责决定“执行 / 跳过 / 记录补偿命令”。
  const plan = await ensureDependencyPlan(step, ctx, state);
  const execute = shouldExecuteStep(step.options);
  const baseResult = {
    manager: plan.manager,
    dependencies: plan.dependencies,
    devDependencies: plan.devDependencies,
    scripts: plan.scripts,
    packageJsonPatch: plan.packageJsonPatch,
    installCommands: plan.installCommands,
    templates: plan.templates,
  };

  if (plan.installCommands.length === 0) {
    state.installedCommands = [];
    const result = {
      ...baseResult,
      executed: false,
      skipped: true,
      reason: 'no dependency install commands were produced',
    };
    state.installDependenciesResult = result;
    registerTransactionOperation(state.transaction, {
      step: readTransactionLabel(step.options, 'installDependencies'),
      target: readTransactionTarget(step.options, plan.manager),
      status: 'skipped',
      rollback: { kind: 'none', reason: 'no dependency install commands were produced' },
    });
    return result;
  }

  if (!execute) {
    state.installedCommands = plan.installCommands.map((command) => ({
      program: command.program,
      args: [...command.args],
      exitCode: 0,
    }));
    const result = {
      ...baseResult,
      executed: false,
      skipped: true,
      reason: 'step execution disabled',
    };
    state.installDependenciesResult = result;
    registerTransactionOperation(state.transaction, {
      step: readTransactionLabel(step.options, 'installDependencies'),
      target: readTransactionTarget(step.options, plan.manager),
      status: 'planned',
      rollback: { kind: 'none', reason: 'step execution disabled' },
    });
    return result;
  }

  const executedCommands: Array<{ program: string; args: string[]; exitCode: number }> = [];
  for (const command of plan.installCommands) {
    const result = await ctx.tools.exec.runChecked({
      program: command.program,
      args: [...command.args],
      cwd: ctx.cwd,
    });
    executedCommands.push({
      program: command.program,
      args: [...command.args],
      exitCode: result.exitCode,
    });
  }

  state.installedCommands = executedCommands;
  const result = {
    ...baseResult,
    executed: true,
    executedCommands,
  };
  state.installDependenciesResult = result;
  registerTransactionOperation(
    state.transaction,
    resolveTransactionRegistration(step.options, {
      step: 'installDependencies',
      target: plan.manager,
      status: 'applied',
      rollback: {
        kind: 'compensation',
        reason: 'dependency rollback requires uninstall compensation commands',
        plan: {
          commands: await createInstallDependencyCompensationCommands(plan, ctx),
          notes: ['Review package.json and lockfile changes before running compensation commands.'],
        },
      },
    }),
  );
  return result;
}

export async function executeGitInit(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // gitInit 既支持“真的执行”，也支持仅返回计划命令。
  // `skipIfExists=true` 时，如果仓库已经初始化，会直接按跳过处理。
  const alreadyInitialized = await ctx.tools.git.git.isInit(ctx.cwd);
  const command = await ctx.tools.git.plan.init();
  const skipIfExists = readBooleanOption(step.options, 'skipIfExists') ?? true;

  if (alreadyInitialized && skipIfExists) {
    state.gitStatus = 'already_initialized';
    const result = {
      executed: false,
      skipped: true,
      alreadyInitialized: true,
      command,
      status: 'already_initialized',
      reason: 'git repository already initialized',
    };
    state.gitInitResult = result;
    registerTransactionOperation(state.transaction, {
      step: readTransactionLabel(step.options, 'gitInit'),
      target: readTransactionTarget(step.options, 'git repository'),
      status: 'skipped',
      rollback: { kind: 'none', reason: 'git repository already initialized' },
    });
    return result;
  }

  if (!shouldExecuteStep(step.options)) {
    state.gitStatus = alreadyInitialized ? 'already_initialized' : 'planned';
    const result = {
      executed: false,
      skipped: true,
      alreadyInitialized,
      command,
      status: state.gitStatus,
      reason: 'step execution disabled',
    };
    state.gitInitResult = result;
    registerTransactionOperation(state.transaction, {
      step: readTransactionLabel(step.options, 'gitInit'),
      target: readTransactionTarget(step.options, 'git repository'),
      status: 'planned',
      rollback: { kind: 'none', reason: 'step execution disabled' },
    });
    return result;
  }

  await ctx.tools.git.git.init(ctx.cwd);
  state.gitStatus = 'initialized';
  const result = {
    executed: true,
    skipped: false,
    alreadyInitialized,
    command,
    status: 'initialized',
  };
  state.gitInitResult = result;
  registerTransactionOperation(
    state.transaction,
    resolveTransactionRegistration(step.options, {
      step: 'gitInit',
      target: 'git repository',
      status: 'applied',
      rollback: {
        kind: 'compensation',
        reason: 'git initialization rollback requires manual cleanup',
        plan: {
          notes: [
            'Remove the generated .git directory only if it was created by this workflow and no extra history was added.',
          ],
        },
      },
    }),
  );
  return result;
}

export async function executePostActions(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // post actions 有两种来源：
  // 1. scaffold plan 中已经解析好的 `postActions`
  // 2. workflow step 上的布尔开关（installDependencies/gitInit/printSummary）
  //
  // 如果显式给了 action 名单，则按名单执行；否则按布尔开关回退。
  const actionNames = readActionNames(step.options);
  const actions: Array<Record<string, unknown>> = [];
  const plannedActions = resolvePostActions(ctx.scaffold.postActions, actionNames);

  if (plannedActions.length > 0) {
    for (const action of plannedActions) {
      actions.push(await executePlannedPostAction(action, step, ctx, state));
    }
    state.postActionResults = actions.map((action) => ({ ...action }));
    return {
      source: actionNames ? 'workflow.options.actions' : 'scaffold.postActions',
      actions,
    };
  }

  const runInstallDependencies = readBooleanOption(step.options, 'installDependencies') ?? true;
  const runGitInit = readBooleanOption(step.options, 'gitInit') ?? true;
  const includeSummary = readBooleanOption(step.options, 'printSummary') ?? false;

  if (runInstallDependencies) {
    actions.push({
      name: 'installDependencies',
      ...(await executeInstallDependencies(step, ctx, state)),
    });
  }
  if (runGitInit) {
    actions.push({
      name: 'gitInit',
      ...(await executeGitInit(step, ctx, state)),
    });
  }
  if (includeSummary) {
    actions.push({
      name: 'printSummary',
      ...(await executePrintSummary(step, ctx, state)),
    });
  }
  state.postActionResults = actions.map((action) => ({ ...action }));
  return {
    source: 'workflow.options.flags',
    actions,
  };
}

export async function executePrintSummary(
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // summary 的计算不依赖 logger 是否存在。
  // 即使某些测试/嵌入场景没有日志能力，这里仍返回完整 summary 供上层消费。
  const summary = createWorkflowSummary(ctx, state);
  const logInfo = ctx.tools.log?.info;
  // 某些测试或嵌入场景不会提供 logger。
  // 这时仍返回 summary，只跳过实际日志输出。
  if (readBooleanOption(step.options, 'emit') !== false && typeof logInfo === 'function') {
    for (const line of summary.lines) {
      await logInfo(line, { target: 'schema.workflow' });
    }
  }
  return summary;
}

function resolvePostActions(
  planned: ScaffoldPostActionPlan[],
  actionNames?: string[],
): ScaffoldPostActionPlan[] {
  // action 名单只做“按名字过滤并保持声明顺序”，不重新排序也不做模糊匹配。
  if (!actionNames || actionNames.length === 0) {
    return planned;
  }
  const lookup = new Map(planned.map((action) => [action.name, action]));
  return actionNames
    .map((name) => lookup.get(name))
    .filter((action): action is ScaffoldPostActionPlan => Boolean(action));
}

async function executePlannedPostAction(
  action: ScaffoldPostActionPlan,
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // 这里把声明式 post action 分发到真正的执行器上，
  // 让调用方不需要了解每种 action 类型背后的具体实现。
  switch (action.type) {
    case 'install_dependencies':
      return {
        name: action.name,
        type: action.type,
        ...(await executeInstallDependencies(step, ctx, state)),
      };
    case 'git_init':
      return {
        name: action.name,
        type: action.type,
        ...(await executeGitInit(step, ctx, state)),
      };
    case 'git_first_commit':
      return {
        name: action.name,
        type: action.type,
        ...(await executeGitFirstCommit(action, step, ctx, state)),
      };
    case 'print_summary':
      return {
        name: action.name,
        type: action.type,
        ...(await executePrintSummary(step, ctx, state)),
      };
    default:
      return assertNever(action.type);
  }
}

async function executeGitFirstCommit(
  action: ScaffoldPostActionPlan,
  step: WorkflowStepDefinition,
  ctx: DynamicCommandContext,
  state: WorkflowExecutionState,
): Promise<Record<string, unknown>> {
  // git_first_commit 的语义是“确保仓库可提交，然后把当前工作区状态作为第一条提交落下去”。
  // 它不会试图自动判断提交内容是否合理，回滚也只提供人工审阅型补偿说明。
  const message = action.message ?? 'chore: scaffold project';
  if (!shouldExecuteStep(step.options)) {
    const result = {
      executed: false,
      skipped: true,
      message,
      reason: 'step execution disabled',
    };
    registerTransactionOperation(state.transaction, {
      step: action.name,
      target: message,
      status: 'planned',
      rollback: { kind: 'none', reason: 'step execution disabled' },
    });
    return result;
  }

  if (!(await ctx.tools.git.git.isInit(ctx.cwd))) {
    await ctx.tools.git.git.init(ctx.cwd);
  }

  const hasChanges = await ctx.tools.git.workspace.hasChanges(ctx.cwd);
  if (!hasChanges) {
    const result = {
      executed: false,
      skipped: true,
      message,
      reason: 'workspace has no changes to commit',
    };
    registerTransactionOperation(state.transaction, {
      step: action.name,
      target: message,
      status: 'skipped',
      rollback: { kind: 'none', reason: 'workspace has no changes to commit' },
    });
    return result;
  }

  await ctx.tools.git.stage.addAll(ctx.cwd);
  await ctx.tools.git.workspace.commit(message, ctx.cwd);
  const result = {
    executed: true,
    skipped: false,
    message,
  };
  registerTransactionOperation(
    state.transaction,
    resolveTransactionRegistration(step.options, {
      step: action.name,
      target: message,
      status: 'applied',
      rollback: {
        kind: 'compensation',
        reason: 'git commit rollback requires manual review',
        plan: {
          notes: ['If needed, revert or reset the generated commit manually after reviewing local changes.'],
        },
      },
    }),
  );
  return result;
}

async function createInstallDependencyCompensationCommands(
  plan: ScaffoldDependencyPlanResult,
  ctx: DynamicCommandContext,
): Promise<Array<{ program: string; args: string[] }>> {
  // 依赖安装的补偿策略不是恢复 lockfile，而是尽量生成一组 remove 命令，
  // 让事务回滚时至少能把新增依赖从 package manager 视角撤下来。
  const packageNames = uniqueStrings([
    ...Object.keys(plan.packageJsonPatch.dependencies),
    ...Object.keys(plan.packageJsonPatch.devDependencies),
  ]);
  if (packageNames.length === 0) {
    return [];
  }
  return [await ctx.tools.pm.command.remove(packageNames, { manager: plan.manager, cwd: ctx.cwd })];
}
