import { resolve } from 'node:path';

import type { ProductPublishManifest } from '../types.js';

import { parseNonNegativeInteger } from '../utils.js';

export type NormalizedPublishOptions = {
  cwd: string;
  packRoot: string;
  outputRoot: string;
  clean: boolean;
  executePublish: boolean;
  dryRun: boolean;
  resume: boolean;
  requireYes: boolean;
  maxRetries: number;
  retryDelayMs: number;
  rollbackOnFailure: boolean;
  otp: string | null;
  npmBin: string | null;
  packDir: string;
  outputDir: string;
};

// 这一层只处理“发布参数归一化”，不触碰任何文件系统副作用。
// 这样调用方可以先得到一份稳定、可复用的配置快照，再决定是否继续进入
// 产物准备、manifest 生成、真正 publish 执行等后续阶段。
export function normalizePublishOptions(params: Record<string, unknown>): NormalizedPublishOptions {
  // cwd 是所有相对路径解析的锚点；如果请求里没有显式传入，就退回当前进程目录。
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  // packDir 表示 pack 阶段已经准备好的安装根目录；
  // outputDir 表示 publish 阶段最终生成 npm 包内容的输出目录。
  // 两者分开保存，后续既能引用原始相对路径，也能直接使用绝对路径。
  const packDir =
    typeof params.packDir === 'string' && params.packDir.trim().length > 0
      ? params.packDir.trim()
      : '.lania/pack/product/install-root';
  const outputDir =
    typeof params.outputDir === 'string' && params.outputDir.trim().length > 0
      ? params.outputDir.trim()
      : '.lania/publish/product/npm-package';

  return {
    cwd,
    packRoot: resolve(cwd, packDir),
    outputRoot: resolve(cwd, outputDir),
    clean: params.clean !== false,
    executePublish: params.execute === true,
    dryRun: params.dryRun === true,
    resume: params.resume === true,
    requireYes: params.yes === true,
    maxRetries: parseNonNegativeInteger(params.maxRetries, 0),
    retryDelayMs: parseNonNegativeInteger(params.retryDelayMs, 1_000),
    rollbackOnFailure: params.rollbackOnFailure === true,
    otp: typeof params.otp === 'string' ? params.otp.trim() : null,
    npmBin:
      typeof params.npmBin === 'string' && params.npmBin.trim().length > 0 ? params.npmBin.trim() : null,
    packDir,
    outputDir,
  };
}

// 真正执行 publish 时，并不需要完整的 NormalizedPublishOptions。
// 这里单独构造执行请求，目的是把“阶段内需要的字段”收缩成最小闭包，
// 防止执行器反向依赖产物准备阶段的其他实现细节。
export function createExecutionRequest(
  manifestPath: string,
  reportPath: string,
  options: NormalizedPublishOptions,
): {
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
} {
  return {
    cwd: options.outputRoot,
    manifestPath,
    reportPath,
    dryRun: options.dryRun,
    otp: options.otp,
    npmBin: options.npmBin,
    yes: options.requireYes,
    maxRetries: options.maxRetries,
    retryDelayMs: options.retryDelayMs,
    rollbackOnFailure: options.rollbackOnFailure,
  };
}

// 初始 execution 状态由外层注入工厂函数来创建，
// 这样当前文件只负责组装 publish 语义所需的输入，
// 不需要了解 manifest 结构内部如何表达 retry policy 和 execution state。
export function createInitialExecution(
  createInitialExecutionState: (
    dryRun: boolean,
    completedSteps: string[],
    retryPolicy: NonNullable<ProductPublishManifest['execution']>['retryPolicy'],
  ) => NonNullable<ProductPublishManifest['execution']>,
  createRetryPolicy: (options: {
    maxRetries: number;
    retryDelayMs: number;
  }) => NonNullable<ProductPublishManifest['execution']>['retryPolicy'],
  options: NormalizedPublishOptions,
  completedSteps: string[],
) {
  return createInitialExecutionState(
    options.dryRun,
    completedSteps,
    createRetryPolicy({ maxRetries: options.maxRetries, retryDelayMs: options.retryDelayMs }),
  );
}
