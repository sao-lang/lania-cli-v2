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
import type { BridgePlugin } from './bridge-plugin.js';
import { registerLocalBridgeHandler, type LocalBridgeHandlerResult } from './local-dispatch.js';

export type SliceLoadResult = {
  plugins: BridgePlugin[];
  rejected: Array<{ plugin: string; reason: string }>;
};

/**
 * 切片加载器表：键 = 切片名（`lania.package.json` 的 `payload.slices` 项）。
 *
 * 阶段 4b：切片已是独立 npm 包，这里改用包名动态 import（B.2 形态）。
 * 说明：动态 import 的 specifier 用变量承载，是刻意为之——核与切片互为
 * devDependency 会造成构建顺序环；核的构建不要求切片 dist 已存在，
 * 缺失切片在运行期按「加载失败 → rejected」路径处理。
 * **插入顺序即 `'*'` 全量装配顺序**，必须与迁移前 plugin-registry.ts 的
 * `builtinPlugins` 数组顺序完全一致（影响握手 methods 排列）。
 */
const SLICE_MODULE_SPECIFIERS: Record<string, string> = {
  config: '@lania-cli-v2/bridge-slice-config',
  'dynamic-commands': '@lania-cli-v2/bridge-slice-dynamic-commands',
  lifecycle: '@lania-cli-v2/bridge-slice-lifecycle',
  compiler: '@lania-cli-v2/bridge-slice-compiler',
  product: '@lania-cli-v2/bridge-slice-product',
  lint: '@lania-cli-v2/bridge-slice-lint',
  system: '@lania-cli-v2/bridge-slice-system',
  template: '@lania-cli-v2/bridge-slice-template',
  // 运行时知识单点：Rust / doctor / 各语言 bridge 都从这里读，不再各自维护副本。
  toolchain: '@lania-cli-v2/bridge-slice-toolchain',
  commitizen: '@lania-cli-v2/bridge-slice-commitizen',
  commitlint: '@lania-cli-v2/bridge-slice-commitlint',
  // agent 切片内置 @lania-cli-v2/agent 的边界适配（切片包内完成适配）。
  agent: '@lania-cli-v2/bridge-slice-agent',
};

export const SLICE_LOADERS: Record<string, () => Promise<BridgePlugin>> = Object.fromEntries(
  Object.entries(SLICE_MODULE_SPECIFIERS).map(([name, specifier]) => [
    name,
    async () => {
      const mod = (await import(specifier)) as { plugin: BridgePlugin };
      return mod.plugin;
    },
  ]),
);

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
      const plugin = await loader();
      // 阶段 4b 依赖倒置：切片加载后把方法前缀注册进核侧本地分发表，
      // 供 schema-tools 的 `tools.bridge` / scaffold 本地路径使用。
      registerLocalBridgeHandler(
        (method, params, context) =>
          plugin.handle(method, params, context) as Promise<LocalBridgeHandlerResult | null>,
        plugin.methods,
      );
      plugins.push(plugin);
    } catch (error) {
      rejected.push({
        plugin: name,
        reason: error instanceof Error ? error.message : String(error),
      });
    }
  }
  return { plugins, rejected };
}
