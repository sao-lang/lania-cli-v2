import { resolve } from 'node:path';

import type { BridgeEvent } from '../../protocol/events.js';
import { asRecord, loadLanConfig, loadPackageJsonSnapshot } from '../../core/runtime.js';
import { createSchemaTools } from '../../core/schema-tools.js';

import { resolveDynamicRuntimeRoots } from './runtime-context.js';
import { resolveScaffoldPlan } from './scaffold.js';
import type {
  DynamicCommandContext,
  DynamicInvocationTarget,
  ProductContext,
  RuntimeContext,
} from './types.js';

// 这一层专门负责把“执行动态命令前需要准备的上下文”一次性组装完整。
// 拆分出来之后，`execute.ts` 可以只关注目标路由、错误包装和最终结果整形，
// 不再同时承担配置读取、产品信息推导、脚手架计划解析、工具集构建等准备工作。
export async function prepareDynamicCommandExecution(
  cwd: string,
  params: Record<string, unknown>,
  target: Extract<DynamicInvocationTarget, { kind: 'manifest_command' }>,
) {
  // 运行时根目录的推导是后续所有上下文的基础：
  // - invocationCwd: 用户本次调用发生的位置
  // - workspaceRoot: 动态命令应当感知的工作区根
  // - productRoot: 产品配置、模板和 package 信息的来源
  // 这里统一解析后，后续逻辑就不需要再次重复判断 installed/development 模式。
  const runtimeRoots = resolveDynamicRuntimeRoots(cwd, params);
  // 动态命令的 argv 来自 bridge 请求，结构是弱类型的 unknown。
  // 这里先统一收敛为 record，避免在后面的 scaffold/runtime/product 构建过程中
  // 到处散落 defensive check。
  const argv = asRecord(params.argv);
  const args = asRecord(argv.args);
  const options = asRecord(argv.options);
  const traceId = typeof params.traceId === 'string' ? params.traceId : null;
  const handlerId = typeof params.handlerId === 'string' ? params.handlerId : undefined;
  // 动态命令既要读取产品配置，也要感知当前运行时布局，因此这里分别构造
  // runtime/product 两个上下文，而不是把所有字段混在一个大对象里。
  const loadedConfig = await loadLanConfig(runtimeRoots.productRoot);
  const runtime = createRuntimeContext(target, traceId, runtimeRoots);
  const product = await createProductContext(runtime, loadedConfig);
  // scaffold 计划来源于命令参数与 schema 中声明的 scaffold 定义。
  // 执行器只消费已经解析好的 scaffold 结果，不需要了解具体的入参兼容细节。
  const scaffold = resolveScaffoldPlan({
    args,
    options,
    spec: target.scaffold,
  });
  const toolEvents: BridgeEvent[] = [];
  // schema tools 在这里统一创建，目的是让后续不同执行路径看到的是同一份
  // runtime/product/scaffold 视图，同时把工具产生的事件集中收集到 toolEvents，
  // 由上层在执行结束后统一并回 bridge 结果。
  const tools = createSchemaTools(
    {
      cwd: runtime.workspaceRoot,
      traceId,
      mount: target.mount,
      path: target.path,
      commandHandlerId: handlerId,
      events: toolEvents,
    },
    {
      scaffold,
      runtime,
      product,
    },
  );
  // DynamicCommandContext 是真正传给执行器的核心对象。
  // 这里把执行阶段需要反复读取的数据提前摊平，后续 handler 不必再回头访问原始 params。
  const context: DynamicCommandContext = {
    cwd: runtime.workspaceRoot,
    mount: target.mount,
    path: target.path,
    argv: { args, options },
    traceId,
    tools,
    scaffold,
    product,
    runtime,
  };

  return {
    context,
    toolEvents,
  };
}

// ProductContext 表示“产品本身”的静态视图，例如名称、版本、模板目录等。
// 它和 RuntimeContext 分离的原因是：
// - RuntimeContext 关心的是本次调用发生在什么运行时布局下
// - ProductContext 关心的是产品元信息本身是什么
// 这样后续工具和执行器可以只依赖自己真正需要的那一侧。
async function createProductContext(
  runtime: RuntimeContext,
  loadedConfig: Awaited<ReturnType<typeof loadLanConfig>>,
): Promise<ProductContext> {
  const productConfig = asRecord(loadedConfig.config.product);
  const name = typeof productConfig.name === 'string' ? productConfig.name : '';
  const binaryName = typeof productConfig.binaryName === 'string' ? productConfig.binaryName : '';
  const displayName =
    typeof productConfig.displayName === 'string' ? productConfig.displayName : null;
  const version = await resolveProductVersion(runtime.productRoot, productConfig);
  const templatesDir =
    typeof productConfig.templatesDir === 'string'
      ? resolve(runtime.productRoot, productConfig.templatesDir)
      : null;

  return {
    name,
    binaryName,
    displayName,
    version,
    productRoot: runtime.productRoot,
    schemaRoot: runtime.schemaRoot,
    templatesDir,
  };
}

// RuntimeContext 只保留与“本次命令运行环境”直接相关的字段，
// 避免把 target、config、schema 等其他层的信息揉进来，维持上下文边界清晰。
function createRuntimeContext(
  target: Extract<DynamicInvocationTarget, { kind: 'manifest_command' }>,
  traceId: string | null,
  runtimeRoots: ReturnType<typeof resolveDynamicRuntimeRoots>,
): RuntimeContext {
  return {
    mode: runtimeRoots.mode,
    traceId,
    invocationCwd: runtimeRoots.invocationCwd,
    workspaceRoot: runtimeRoots.workspaceRoot,
    productRoot: runtimeRoots.productRoot,
    schemaRoot: target.schemaRoot,
  };
}

// 产品版本允许来自显式配置，也允许按策略从 package.json 回填。
// 在这里集中处理版本解析后，后续发布/模板/脚手架相关逻辑都可以直接消费统一结果。
async function resolveProductVersion(
  productRoot: string,
  productConfig: Record<string, unknown>,
): Promise<string | null> {
  if (typeof productConfig.version === 'string') {
    return productConfig.version;
  }

  if (productConfig.versionStrategy !== 'package_json') {
    return null;
  }

  const packageJson = await loadPackageJsonSnapshot(productRoot);
  return typeof packageJson.version === 'string' ? packageJson.version : null;
}

// 动态命令的不同执行器可能返回 `exitCode` 或 `exit_code`。
// 这里做一次兼容归一化，避免调用方在多个地方重复处理历史字段差异。
export function asExitCode(value: unknown): number {
  const record = asRecord(value);
  const exitCode =
    typeof record.exitCode === 'number'
      ? record.exitCode
      : typeof record.exit_code === 'number'
        ? record.exit_code
        : 0;
  return Number.isFinite(exitCode) ? exitCode : 0;
}
