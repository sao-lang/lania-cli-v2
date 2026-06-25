import type { BridgePluginResult } from '../../core/bridge-plugin.js';
import type { BridgeEvent } from '../../protocol/events.js';
import { asRecord, loadLanConfig } from '../../core/runtime.js';
import { createDefaultPluginSecurityPolicy } from '../../core/plugin-policy.js';

import { getLocalExecutor } from './state.js';
import { loadCommandPlugin, mergePluginDeclarations } from './plugin-runtime.js';
import type {
  CommandHandlerSpec,
  DynamicCommandContext,
  DynamicInvocationTarget,
} from './types.js';

// 这一层只负责“怎么执行”，不再负责准备上下文。
// 经过拆分后，调用顺序变成：
// 1. `execute-context.ts` 负责装配运行所需的上下文
// 2. 当前文件根据 target.executor 选择真正的执行路径
// 3. `execute.ts` 只负责总入口编排和结果收口
export async function runDynamicExecutor(
  cwd: string,
  target: Extract<DynamicInvocationTarget, { kind: 'manifest_command' }>,
  ctx: DynamicCommandContext,
): Promise<BridgePluginResult<Record<string, unknown>>> {
  const executor = target.executor;
  // local executor 表示处理逻辑已经在当前 node-bridge 进程内注册完成，
  // 因此直接通过内存态 registry 取出 handler 执行，不需要再走插件加载。
  if (executor.type === 'local') {
    const handler = getLocalExecutor(cwd, executor.id);
    if (!handler) {
      throw new Error(`local handler not found: ${executor.id}`);
    }
    return normalizeHandlerOutcome(await handler(ctx));
  }
  // 非 local 场景走 manifest plugin handler。
  // 这里会把 schema/配置里声明的插件信息和执行上下文一起交给插件运行时处理。
  return invokeManifestHandler(
    cwd,
    { plugin: executor.plugin, method: executor.method },
    { ...ctx, declaredPlugins: target.declaredPlugins ?? [] },
  );
}

// 历史上本地执行器可能只返回纯对象，也可能返回 `{ result, events }` 结构。
// 这里把两种返回形式归一化成统一的 BridgePluginResult，避免上层再写分支。
function normalizeHandlerOutcome(out: unknown): BridgePluginResult<Record<string, unknown>> {
  if (out && typeof out === 'object') {
    const record = out as Record<string, unknown>;
    if ('result' in record && Array.isArray(record.events)) {
      return {
        result: asRecord(record.result),
        events: record.events as BridgeEvent[],
      };
    }
  }
  return { result: asRecord(out), events: [] };
}

// manifest handler 的职责是把“逻辑位于某个插件里”的执行请求转交给插件运行时。
// 这里做的事情有三步：
// 1. 重新读取配置，拿到项目声明的插件列表
// 2. 合并 schema 声明和项目配置声明，定位本次要执行的插件
// 3. 基于安全策略加载插件，并调用对应 method
async function invokeManifestHandler(
  cwd: string,
  handler: CommandHandlerSpec,
  params: Record<string, unknown>,
): Promise<BridgePluginResult<Record<string, unknown>>> {
  const loaded = await loadLanConfig(cwd);
  // target 上的 declaredPlugins 通常来自 schema 或动态命令声明，
  // merge 后允许“项目配置已有声明”和“命令声明临时附带插件”两种来源协同工作。
  const declarations = mergePluginDeclarations(loaded.config.plugins, params.declaredPlugins);
  const declaration = declarations.find(
    (candidate) => candidate.package === handler.plugin || candidate.name === handler.plugin,
  );
  if (!declaration) {
    throw new Error(`plugin \`${handler.plugin}\` is not declared in config or schema plugins`);
  }

  const policy = createDefaultPluginSecurityPolicy(loaded.config);
  const plugin = await loadCommandPlugin(cwd, declaration, policy, handler.method);
  const handled = await plugin.handle(handler.method, params, { cwd });
  // 插件如果显式返回 handled 结果，则按插件返回值透传；
  // 如果插件只表示“我接收了这个调用”，则构造一个最小 accepted 响应，
  // 这样上层可以稳定拿到结构化结果，而不是处理 undefined。
  if (handled) {
    return {
      result: asRecord(handled.result),
      events: handled.events,
    };
  }
  return {
    result: {
      accepted: true,
      plugin: declaration.package,
      method: handler.method,
    },
    events: [],
  };
}
