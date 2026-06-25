import { spawnSync } from 'node:child_process';
import fs from 'node:fs';

import { asRecord } from '../../../core/runtime.js';

import { writeJsonFile } from '../fs.js';
import type { ProductDistributionReport, ProductPublishManifest, ProductPublishManifestStep } from '../types.js';
import { sleepMs } from '../utils.js';
import { resolveExecutionTarballPath } from './resolve.js';

// 负责真正执行 publish manifest：
// - 消费已经规划好的发布步骤
// - 逐步执行 `npm publish`，并在需要时做有限重试
// - 把执行进度持续写回 manifest 和 report，保证中断后可以 resume

export function createRetryPolicy(options: {
  maxRetries: number;
  retryDelayMs: number;
}): NonNullable<ProductPublishManifest['execution']>['retryPolicy'] {
  return {
    maxRetries: options.maxRetries,
    retryDelayMs: options.retryDelayMs,
  };
}

export function createInitialExecutionState(
  dryRun: boolean,
  completedSteps: string[] = [],
  retryPolicy: NonNullable<ProductPublishManifest['execution']>['retryPolicy'] = {
    maxRetries: 0,
    retryDelayMs: 1_000,
  },
): NonNullable<ProductPublishManifest['execution']> {
  // 执行状态需要同时描述“进行到哪一步”和“失败后怎么恢复/回滚”。
  // 这里统一给出一个可持久化的初始结构，避免上层零散拼字段。
  return {
    executed: false,
    dryRun,
    completedSteps: [...completedSteps],
    resumed: completedSteps.length > 0,
    failedStepId: null,
    lastError: null,
    attempts: [],
    retryPolicy,
    rollbackPlan: {
      status: 'not_needed',
      generatedAt: null,
      reason: null,
      commands: [],
    },
    updatedAt: new Date().toISOString(),
  };
}

export function collectResumedSteps(
  previousManifest: ProductPublishManifest | null,
  nextManifest: ProductPublishManifest,
): string[] {
  // 只有新 manifest 中仍然存在的 step 才允许被视为“已完成”并继承下来，
  // 这样可以避免旧计划里的遗留步骤污染新的发布计划。
  const previous = previousManifest?.execution?.completedSteps ?? [];
  if (previous.length === 0) {
    return [];
  }
  const validSteps = new Set(nextManifest.steps.map((entry) => entry.id));
  return previous.filter((entry) => validSteps.has(entry));
}

export async function persistPublishArtifacts(
  manifestPath: string,
  reportPath: string,
  manifest: ProductPublishManifest,
  report: ProductDistributionReport,
): Promise<void> {
  // report 和 manifest 都会被用户当作“恢复现场”的依据，因此两边的 execution 必须保持同步。
  const reportExperimental = asRecord(report.experimental);
  const registryPublish = asRecord(reportExperimental.registryPublish);
  registryPublish.execution = manifest.execution;
  reportExperimental.registryPublish = registryPublish;
  report.experimental = reportExperimental;

  await writeJsonFile(manifestPath, manifest);
  await writeJsonFile(reportPath, report);
}

export async function executePublishManifest(
  manifest: ProductPublishManifest,
  report: ProductDistributionReport,
  options: {
    cwd: string;
    manifestPath: string;
    reportPath: string;
    dryRun: boolean;
    otp: string | null;
    npmBin: string | null;
    yes: boolean;
    maxRetries: number;
    retryDelayMs: number;
    rollbackOnFailure: boolean;
  },
): Promise<NonNullable<ProductPublishManifest['execution']>> {
  // 发布执行主链路：
  // 1. 初始化或恢复 execution state
  // 2. 先做 preflight，把显而易见的失败挡在真正 publish 之前
  // 3. 逐个 step 执行并落盘，保证任意时刻都能 resume
  // 4. 失败时生成 rollback plan，并按配置决定是否自动执行
  const execution =
    manifest.execution ??
    createInitialExecutionState(options.dryRun, [], createRetryPolicy(options));

  // resume 时 CLI 参数可能已变化，所以每次执行都重新刷新 retry policy。
  execution.retryPolicy = createRetryPolicy(options);

  const completedThisRun: string[] = [];

  try {
    execution.preflight = runPublishPreflight(manifest, options);
    execution.updatedAt = new Date().toISOString();
    execution.lastError = null;
    await persistPublishArtifacts(options.manifestPath, options.reportPath, manifest, report);
  } catch (error) {
    execution.lastError = error instanceof Error ? error.message : String(error);
    execution.updatedAt = new Date().toISOString();
    await persistPublishArtifacts(options.manifestPath, options.reportPath, manifest, report);
    throw error;
  }

  for (const step of manifest.steps) {
    if (execution.completedSteps.includes(step.id)) {
      continue;
    }

    try {
      await runNpmPublishStepWithRetry(execution, step, {
        cwd: options.cwd,
        dryRun: options.dryRun,
        otp: options.otp,
        npmBin: options.npmBin,
        maxRetries: options.maxRetries,
        retryDelayMs: options.retryDelayMs,
      });

      execution.completedSteps.push(step.id);
      completedThisRun.push(step.id);
      execution.failedStepId = null;
      execution.lastError = null;
      execution.rollbackPlan = createExecutionRollbackPlan(manifest, completedThisRun);
      execution.updatedAt = new Date().toISOString();
      await persistPublishArtifacts(options.manifestPath, options.reportPath, manifest, report);
    } catch (error) {
      execution.failedStepId = step.id;
      execution.lastError = error instanceof Error ? error.message : String(error);
      execution.rollbackPlan = createExecutionRollbackPlan(manifest, completedThisRun);

        // rollback 是可选的 best-effort 行为，dry-run 下不会真的执行。
      if (options.rollbackOnFailure && !options.dryRun && execution.rollbackPlan.commands.length > 0) {
        try {
          executeNpmRollbackPlan(execution.rollbackPlan, {
            cwd: options.cwd,
            npmBin: options.npmBin,
          });
        } catch (rollbackError) {
          execution.rollbackPlan.status = 'failed';
          execution.rollbackPlan.reason =
            rollbackError instanceof Error ? rollbackError.message : String(rollbackError);
        }
      }

      execution.updatedAt = new Date().toISOString();
      await persistPublishArtifacts(options.manifestPath, options.reportPath, manifest, report);
      throw error;
    }
  }

  execution.executed = true;
  execution.failedStepId = null;
  execution.lastError = null;
  execution.rollbackPlan = createExecutionRollbackPlan(manifest, []);
  execution.updatedAt = new Date().toISOString();
  await persistPublishArtifacts(options.manifestPath, options.reportPath, manifest, report);

  return execution;
}

function runNpmPublishStep(
  step: ProductPublishManifestStep,
  options: {
    cwd: string;
    dryRun: boolean;
    otp: string | null;
    npmBin: string | null;
  },
): string[] {
  // 单个 step 的职责很纯粹：补齐执行期开关参数，然后同步调用 npm publish。
  const args = [...step.command.args];
  if (options.dryRun && !args.includes('--dry-run')) {
    args.push('--dry-run');
  }
  if (options.otp && !args.includes('--otp')) {
    args.push('--otp', options.otp);
  }
  const result = spawnSync(options.npmBin ?? step.command.program, args, {
    cwd: options.cwd,
    encoding: 'utf8',
    env: process.env,
  });
  if (result.status !== 0) {
    throw new Error(
      `${step.command.program} ${args.join(' ')} failed for ${step.packageName}: ${
        result.stderr || result.stdout || 'unknown error'
      }`,
    );
  }
  return args;
}

function isRetriablePublishFailure(message: string): boolean {
  // 仅把网络波动、限流、临时服务异常视为可重试；
  // 权限、配置、版本冲突这类确定性失败不应该反复重放。
  return /EAI_AGAIN|ECONNRESET|ECONNREFUSED|ETIMEDOUT|ECONNABORTED|ENOTFOUND|EPIPE|socket hang up|503|502|504|429|rate limit|network/i.test(
    message,
  );
}

async function runNpmPublishStepWithRetry(
  execution: NonNullable<ProductPublishManifest['execution']>,
  step: ProductPublishManifestStep,
  options: {
    cwd: string;
    dryRun: boolean;
    otp: string | null;
    npmBin: string | null;
    maxRetries: number;
    retryDelayMs: number;
  },
): Promise<void> {
  // 这里实现的是“最多 N 次尝试，且只有错误可重试才继续”的策略。
  // 每次尝试都会落进 attempts，便于后续审计和恢复时判断失败历史。
  const maxAttempts = options.maxRetries + 1;
  let attempt = 0;
  while (attempt < maxAttempts) {
    attempt += 1;
    const startedAt = new Date().toISOString();
    try {
      const args = runNpmPublishStep(step, options);
      execution.attempts.push({
        stepId: step.id,
        packageName: step.packageName,
        attempt,
        status: 'succeeded',
        retriable: false,
        startedAt,
        finishedAt: new Date().toISOString(),
        args,
        error: null,
      });
      return;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const retriable = attempt < maxAttempts && isRetriablePublishFailure(message);
      execution.attempts.push({
        stepId: step.id,
        packageName: step.packageName,
        attempt,
        status: 'failed',
        retriable,
        startedAt,
        finishedAt: new Date().toISOString(),
        args: [...step.command.args],
        error: message,
      });
      if (!retriable) {
        throw error;
      }
      await sleepMs(options.retryDelayMs);
    }
  }
}

function runPublishPreflight(
  manifest: ProductPublishManifest,
  options: {
    cwd: string;
    dryRun: boolean;
    npmBin: string | null;
    yes: boolean;
  },
): NonNullable<NonNullable<ProductPublishManifest['execution']>['preflight']> {
  // 真实发布必须显式 `--yes`，避免把试运行命令误打到真实 registry。
  if (!options.dryRun && !options.yes) {
    throw new Error('Real publish execution requires --yes to avoid accidental registry pushes');
  }

  // 先确认每个 step 对应的 tarball 都已经存在，避免真正 publish 时才暴露缺包问题。
  const packageByName = new Map(manifest.packages.map((entry) => [entry.name, entry]));
  let tarballsVerified = 0;
  for (const step of manifest.steps) {
    const tarballPath = resolveExecutionTarballPath(options.cwd, step.tarball);
    if (!fs.existsSync(tarballPath)) {
      throw new Error(`Publish preflight missing tarball for ${step.packageName}: ${step.tarball}`);
    }
    tarballsVerified += 1;
  }

  const registry = manifest.steps[0]?.publishConfig.registry ?? 'https://registry.npmjs.org/';
  const whoami = spawnSync(options.npmBin ?? 'npm', ['whoami', '--registry', registry], {
    cwd: options.cwd,
    encoding: 'utf8',
    env: process.env,
  });
  if (whoami.status !== 0) {
    throw new Error(
      `Publish preflight npm whoami failed for ${registry}: ${
        whoami.stderr || whoami.stdout || 'unknown error'
      }`,
    );
  }
  const actor = whoami.stdout.trim() || null;

  // 如果 registry 上已经有同名同版本，直接在 preflight 阶段拦住，
  // 避免真实发布到一半才发现一部分成功、一部分冲突。
  const versionConflicts: string[] = [];
  for (const step of manifest.steps) {
    const pkg = packageByName.get(step.packageName);
    if (!pkg) {
      continue;
    }
    const view = spawnSync(
      options.npmBin ?? 'npm',
      [
        'view',
        `${step.packageName}@${pkg.version}`,
        'version',
        '--registry',
        step.publishConfig.registry,
      ],
      {
        cwd: options.cwd,
        encoding: 'utf8',
        env: process.env,
      },
    );
    if (view.status === 0 && view.stdout.trim().length > 0) {
      versionConflicts.push(`${step.packageName}@${pkg.version}`);
    }
  }
  if (versionConflicts.length > 0) {
    throw new Error(
      `Publish preflight blocked existing package versions: ${versionConflicts.join(', ')}`,
    );
  }

  return {
    checked: true,
    actor,
    registry,
    tarballsVerified,
    versionConflicts,
  };
}

function createExecutionRollbackPlan(
  manifest: ProductPublishManifest,
  completedStepIds: string[],
): NonNullable<ProductPublishManifest['execution']>['rollbackPlan'] {
  // rollback plan 总是按“已成功发布步骤的逆序”生成，方便失败后尽量按相反顺序撤销。
  if (completedStepIds.length === 0) {
    return {
      status: 'not_needed',
      generatedAt: new Date().toISOString(),
      reason: 'no new packages were published in the failed invocation',
      commands: [],
    };
  }
  const packageByName = new Map(manifest.packages.map((entry) => [entry.name, entry]));
  const commands = completedStepIds
    .map((stepId) => manifest.steps.find((entry) => entry.id === stepId))
    .filter((entry): entry is ProductPublishManifestStep => Boolean(entry))
    .reverse()
    .map((step) => {
      const pkg = packageByName.get(step.packageName);
      return {
        stepId: step.id,
        packageName: step.packageName,
        version: pkg?.version ?? null,
        registry: step.publishConfig.registry,
        command: [
          'unpublish',
          pkg?.version ? `${step.packageName}@${pkg.version}` : step.packageName,
          '--registry',
          step.publishConfig.registry,
        ],
      };
    });
  return {
    status: 'planned',
    generatedAt: new Date().toISOString(),
    reason:
      'publish failed after partial success; review and optionally execute rollback commands in reverse order',
    commands,
  };
}

function executeNpmRollbackPlan(
  rollbackPlan: NonNullable<ProductPublishManifest['execution']>['rollbackPlan'],
  options: {
    cwd: string;
    npmBin: string | null;
  },
): void {
  // 真正执行 rollback 时仍然是 best-effort；
  // 即便失败，也会把计划和失败原因保留下来，供用户人工处理。
  for (const rollback of rollbackPlan.commands) {
    const result = spawnSync(options.npmBin ?? 'npm', rollback.command, {
      cwd: options.cwd,
      encoding: 'utf8',
      env: process.env,
    });
    if (result.status !== 0) {
      throw new Error(
        `${rollback.command.join(' ')} failed for ${rollback.packageName}: ${
          result.stderr || result.stdout || 'unknown error'
        }`,
      );
    }
  }
  rollbackPlan.status = 'executed';
  rollbackPlan.reason = 'rollback commands executed in reverse publish order';
}
