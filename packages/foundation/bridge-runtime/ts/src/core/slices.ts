/**
 * 切片（slice）装配器：把原 `builtinPlugins` 静态数组改为按需动态 import。
 *
 * 主要导出：SLICE_LOADERS、loadSlices、resolveSliceSpec。
 *
 * 设计依据（拆分文档 §5.2 B.2/B.4）：
 * - 切片列表由环境变量 `LANIA_BRIDGE_SLICES` 驱动，值是构建期由 `lania.package.json`
 *   的 `payload.slices` 决定的静态列表；运行时只做 ESM `import()`，不做任何目录扫描。
 * - 这不是「命令动态发现」：切片名在构建期确定，未知切片静默跳过（记入 rejected）。
 * - `agent` 切片经 `agentBridgePlugin` 边界适配：`@lania-cli-v2/agent` 用结构化事件类型，
 *   与 bridge 互不依赖，这里做一次运行时适配（原 plugin-registry.ts:43-57 的逻辑）。
 *
 * 默认值语义（向后兼容，阶段 4a）：
 * - 环境变量**未注入**（undefined）→ 视为 `'*'`（全量），与迁移前静态加载行为逐字节一致；
 *   总包 payload 与历史 Rust 宿主都不注入该变量，因此沿用旧行为。
 * - 空字符串 → 零切片（registry 仍可握手，methods 为空，不加载 agent/ejs/templates）。
 * - 非空 → 逗号分隔的切片名列表。
 */
import type { BridgeEvent } from '../protocol/events.js';
import type { BridgePlugin } from './bridge-plugin.js';

export type SliceLoadResult = {
  plugins: BridgePlugin[];
  rejected: Array<{ plugin: string; reason: string }>;
};

/**
 * `@lania-cli-v2/agent` 的边界适配（原 plugin-registry.ts 内联逻辑）：
 * agent 的流式输出全部走 `emit`，返回值里不带事件，因此这里统一转换。
 */
function adaptAgentPlugin(agentPlugin: BridgePlugin): BridgePlugin {
  return {
    name: agentPlugin.name,
    methods: agentPlugin.methods,
    handle: async (method, params, context) => {
      const result = await agentPlugin.handle(method, params, {
        cwd: context?.cwd ?? null,
        ...(context?.emit ? { emit: context.emit } : {}),
        ...(context?.hostCall ? { hostCall: context.hostCall } : {}),
      });
      if (!result) {
        return undefined;
      }
      return { result: result.result, events: [] as BridgeEvent[] };
    },
  };
}

/**
 * 切片加载器表：键 = 切片名（`lania.package.json` 的 `payload.slices` 项），
 * 值 = 懒加载函数。**插入顺序即 `'*'` 全量装配顺序**，必须与迁移前
 * plugin-registry.ts 的 `builtinPlugins` 数组顺序完全一致（影响握手 methods 排列）。
 */
export const SLICE_LOADERS: Record<string, () => Promise<BridgePlugin>> = {
  config: async () => (await import('../plugins/config.js')).configPlugin,
  'dynamic-commands': async () =>
    (await import('../plugins/dynamic-commands.js')).dynamicCommandsPlugin,
  lifecycle: async () => (await import('../plugins/lifecycle.js')).lifecyclePlugin,
  compiler: async () => (await import('../plugins/compiler.js')).compilerPlugin,
  product: async () => (await import('../plugins/product.js')).productPlugin,
  lint: async () => (await import('../plugins/lint.js')).lintPlugin,
  system: async () => (await import('../plugins/system.js')).systemPlugin,
  template: async () => (await import('../plugins/template.js')).templatePlugin,
  // 运行时知识单点：Rust / doctor / 各语言 bridge 都从这里读，不再各自维护副本。
  toolchain: async () => (await import('../plugins/toolchain-plugin.js')).toolchainPlugin,
  commitizen: async () => (await import('../plugins/commitizen.js')).commitizenPlugin,
  commitlint: async () => (await import('../plugins/commitlint.js')).commitlintPlugin,
  agent: async () =>
    adaptAgentPlugin((await import('@lania-cli-v2/agent')).agentPlugin as BridgePlugin),
};

/** 读取切片装配清单（见模块注释的默认值语义）。 */
export function resolveSliceSpec(): string {
  const raw = process.env.LANIA_BRIDGE_SLICES;
  return raw === undefined ? '*' : raw;
}

/**
 * 按切片清单装配插件：
 * - `'*'` → 全部切片（保持 SLICE_LOADERS 的声明顺序）；
 * - 其它 → 逗号分隔列表，未知切片静默跳过并记入 rejected；
 * - 单个切片加载失败不拖垮整体（记入 rejected，等价于动态插件的 reject 语义）。
 */
export async function loadSlices(spec: string): Promise<SliceLoadResult> {
  const names =
    spec === '*'
      ? Object.keys(SLICE_LOADERS)
      : spec
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean);
  const plugins: BridgePlugin[] = [];
  const rejected: Array<{ plugin: string; reason: string }> = [];
  for (const name of names) {
    const loader = SLICE_LOADERS[name];
    if (!loader) {
      rejected.push({ plugin: name, reason: `unknown slice "${name}"` });
      continue;
    }
    try {
      plugins.push(await loader());
    } catch (error) {
      rejected.push({
        plugin: name,
        reason: error instanceof Error ? error.message : String(error),
      });
    }
  }
  return { plugins, rejected };
}
