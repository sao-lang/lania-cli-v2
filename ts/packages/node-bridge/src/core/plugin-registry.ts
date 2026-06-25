/**
 * 插件注册中心，装配内建插件并按 cwd 加载、缓存项目插件。
 *
 * 主要导出：createPluginRegistry、PluginRegistrySnapshot。
 *
 * 设计目标：
 * - 内建插件（builtin）始终可用，用于承载 bridge 的基础能力（config/template/compiler/lifecycle 等）。
 * - 项目插件（dynamic）由 `lan.config.*` 声明，按 cwd 粒度解析与缓存，避免每次 request 都重复加载。
 * - 对动态插件做“最小权限”裁剪：声明的方法 + 插件导出的方法 + policy.allowedMethods 三者取交集。
 *
 * 两个常用入口的语义差异：
 * - `resolve(cwd)`：会触发读取 lan.config 并加载动态插件（可能产生 rejectedPlugins）
 * - `snapshot(cwd)`：只返回“当前已缓存的结果”；若 cwd 从未 resolve 过，则退回 builtin-only
 */
import { commitizenPlugin } from '../plugins/commitizen.js';
import { commitlintPlugin } from '../plugins/commitlint.js';
import { compilerPlugin } from '../plugins/compiler.js';
import { configPlugin } from '../plugins/config.js';
import { dynamicCommandsPlugin } from '../plugins/dynamic-commands.js';
import { lifecyclePlugin } from '../plugins/lifecycle.js';
import { lintPlugin } from '../plugins/lint.js';
import { productPlugin } from '../plugins/product.js';
import { systemPlugin } from '../plugins/system.js';
import { templatePlugin } from '../plugins/template.js';
import type { BridgePlugin } from './bridge-plugin.js';
import { createBridgeMetrics } from './metrics.js';
import {
  createDefaultPluginSecurityPolicy,
  intersectAllowedMethods,
  validatePluginDeclaration,
  type RuntimePluginDeclaration,
} from './plugin-policy.js';
import { loadLanConfig, resolveModuleFromCwd } from './runtime.js';

const builtinPlugins: BridgePlugin[] = [
  // builtin 插件是 bridge 协议的一部分，顺序不影响功能，但会影响握手时 methods 的排列。
  configPlugin,
  dynamicCommandsPlugin,
  lifecyclePlugin,
  compilerPlugin,
  productPlugin,
  lintPlugin,
  systemPlugin,
  templatePlugin,
  commitizenPlugin,
  commitlintPlugin,
];

// registry 对外只暴露快照字段，但内部缓存条目还需要保留完整 plugin 对象：
// - `resolvePlugin()` 需要回到真正的 `handle()` 实现
// - 同一次 cwd 生命周期里后续 request 分发也会直接复用这批对象
// 这样可以把“可序列化快照”和“运行时插件实例”收敛在一份缓存里，避免双份状态漂移。
type PluginRegistryCacheEntry = PluginRegistrySnapshot & { plugins: BridgePlugin[] };

// reject 记录保持极简形态，刻意只保留“哪个插件、为什么失败”，方便：
// - metrics/diagnostics 展示
// - snapshot 序列化
// - 后续在不泄露过多实现细节的前提下排查配置问题
type RejectedPluginRecord = { plugin: string; reason: string };

export interface PluginRegistrySnapshot {
  /**
   * 插件展示名集合（包含 builtin + dynamic）。
   * 注意：dynamic 插件可以通过 declaration.name 覆盖插件自身导出的 name。
   */
  pluginNames: string[];
  /**
   * 当前允许暴露给 Rust 端的方法集合。
   * 注意：这里已做过 policy 与 declaration methods 的裁剪。
   */
  methods: string[];
  /**
   * 加载被拒绝的插件列表（用于观测/诊断），不影响其它插件正常工作。
   * 常见原因：
   * - 声明格式不合法
   * - policy 不允许的来源/包名
   * - 模块加载失败或导出形态不符合 BridgePlugin
   * - methods 交集为空（等价于“在当前策略下没有任何可用能力”）
   */
  rejectedPlugins: Array<{ plugin: string; reason: string }>;
}

export function createPluginRegistry() {
  const metrics = createBridgeMetrics();
  // 以 cwd 为粒度缓存：同一项目的一次 CLI 调用可能触发多次 resolve。
  // 这里不做全局唯一 registry，是因为动态插件集合天然依赖“项目上下文”。
  const cache = new Map<string, PluginRegistryCacheEntry>();

  // builtin-only 既是“非项目上下文”的最终结果，也是 snapshot() 在未 resolve
  // 过某个 cwd 时的稳定回退值。抽成 helper 可以避免这两个入口出现细微漂移。
  const createBuiltinOnlySnapshot = (): PluginRegistryCacheEntry => ({
    plugins: builtinPlugins,
    pluginNames: builtinPlugins.map((plugin) => plugin.name),
    methods: builtinPlugins.flatMap((plugin) => plugin.methods),
    rejectedPlugins: [] as RejectedPluginRecord[],
  });

  // 完整快照组装保持单点，避免后续新增字段时 builtin-only 与 dynamic 路径出现偏差。
  // 这里不主动去重 methods，保留插件声明顺序，便于握手输出和诊断与真实装配顺序一致。
  const createSnapshot = (
    plugins: BridgePlugin[],
    rejectedPlugins: RejectedPluginRecord[],
  ): PluginRegistryCacheEntry => ({
    plugins,
    pluginNames: plugins.map((plugin) => plugin.name),
    methods: plugins.flatMap((plugin) => plugin.methods),
    rejectedPlugins,
  });

  // 动态插件解析分三步：
  // 1. 规范化声明
  // 2. 通过 policy 校验并加载模块
  // 3. 对 methods 再做一次最小权限裁剪
  // 返回值刻意分成 loaded/rejected 两路，便于主流程保持“先解析，再组装快照”的结构。
  const resolveDynamicPlugins = async (
    cwd: string,
    declaredPlugins: unknown[],
    policy: ReturnType<typeof createDefaultPluginSecurityPolicy>,
  ): Promise<{ dynamicPlugins: BridgePlugin[]; rejectedPlugins: RejectedPluginRecord[] }> => {
    const dynamicPlugins: BridgePlugin[] = [];
    const rejectedPlugins: RejectedPluginRecord[] = [];

    for (const value of declaredPlugins) {
      // 先把用户配置写法统一成 declaration 结构，后续流程就不必区分 string/object 两种来源。
      const declaration = normalizeDeclaration(value);
      if (!declaration) {
        // 声明格式错误：记录 rejectedPlugins 以便 `bridge.metrics` 观测。
        metrics.recordPluginReject();
        rejectedPlugins.push({
          plugin: String(value),
          reason: 'plugin declaration must be string or object',
        });
        continue;
      }

      try {
        // 校验 -> 加载模块 -> 限制可用方法：任何一步失败都记录为 reject。
        // 注意：这里即使加载成功，也会把 plugin.methods 再做一次裁剪（policy + declaration methods）。
        // 目的：最小权限，避免插件暴露额外方法被误调用。
        const validated = validatePluginDeclaration(cwd, declaration, policy);
        const plugin = await loadPluginModule(cwd, validated.resolvedPath);
        const allowedMethods = intersectAllowedMethods(validated.methods, plugin.methods, policy);
        if (allowedMethods.length === 0) {
          // “成功加载但没有可用方法”在 bridge 看来等价于不可用插件，因此仍计为 reject。
          throw new Error('plugin exposes no allowed methods');
        }
        dynamicPlugins.push({
          ...plugin,
          name: validated.name ?? plugin.name,
          methods: allowedMethods,
        });
        metrics.recordPluginLoad();
      } catch (error) {
        metrics.recordPluginReject();
        rejectedPlugins.push({
          plugin: declaration.package,
          reason: error instanceof Error ? error.message : String(error),
        });
      }
    }

    return { dynamicPlugins, rejectedPlugins };
  };

  // `load()` 是 registry 的唯一装配入口：
  // - 决定 builtin-only 还是 builtin + dynamic
  // - 负责缓存命中
  // - 负责动态插件的声明校验、模块加载和最小权限裁剪
  async function load(cwd?: string | null): Promise<PluginRegistryCacheEntry> {
    if (!cwd) {
      // 没有 cwd 表示“非项目上下文”，只加载内建插件。
      return createBuiltinOnlySnapshot();
    }
    if (cache.has(cwd)) {
      // 缓存命中后直接复用快照，确保同一次 CLI 生命周期里插件集合稳定。
      return cache.get(cwd)!;
    }

    // 动态插件只在“项目上下文”中启用：读取 lan.config.plugins 并逐个校验/加载。
    const loaded = await loadLanConfig(cwd);
    const config = loaded.config;
    const policy = createDefaultPluginSecurityPolicy(config);
    const declaredPlugins = Array.isArray(config.plugins) ? config.plugins : [];
    const { dynamicPlugins, rejectedPlugins } = await resolveDynamicPlugins(
      cwd,
      declaredPlugins,
      policy,
    );
    const snapshot = createSnapshot([...builtinPlugins, ...dynamicPlugins], rejectedPlugins);
    // 只缓存成功装配后的最终快照，避免外部观察到“半加载中”的中间态。
    cache.set(cwd, snapshot);
    return snapshot;
  }

  return {
    metrics,
    /**
     * 解析并返回完整插件列表（builtin + dynamic）。
     * 这是“会产生副作用”的入口：可能触发读取 `lan.config.*`、加载动态模块并写入缓存。
     */
    async resolve(cwd?: string | null) {
      return load(cwd);
    },
    /**
     * 给定 request.method，返回第一个声明支持该方法的插件。
     * 当前实现保持线性扫描，原因是 methods 规模通常很小，而且它天然复用 `resolve()`
     * 的动态加载与缓存语义，避免维护第二套索引构建逻辑。
     */
    async resolvePlugin(requestMethod: string, cwd?: string | null) {
      const registry = await load(cwd);
      return registry.plugins.find((plugin) => plugin.methods.includes(requestMethod));
    },
    /**
     * 仅返回当前快照，不触发动态加载。
     * - 若 cwd 已 resolve：返回缓存内容
     * - 若 cwd 未 resolve：返回 builtin-only（rejectedPlugins 为空）
     * 这个入口主要给观测和诊断使用，语义上刻意区别于 `resolve()`。
     */
    snapshot(cwd?: string | null) {
      return cwd && cache.has(cwd) ? cache.get(cwd)! : createBuiltinOnlySnapshot();
    },
  };
}

// 兼容两种声明风格：
// - string: `plugins: ["pkg"]`
// - object: `plugins: [{ package, methods, signature }]`
// 规范化后的结果会继续进入 policy 校验与模块解析流程。
function normalizeDeclaration(value: unknown): RuntimePluginDeclaration | null {
  if (typeof value === 'string') {
    // 简写形式：`plugins: ["xxx"]`
    return { package: value };
  }
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const record = value as Record<string, unknown>;
  // 兼容历史上把 name 当作 package 写的宽松配置；若同时存在 package/name，
  // package 仍是模块解析来源，name 只用于覆盖展示名。
  const packageName =
    typeof record.package === 'string'
      ? record.package
      : typeof record.name === 'string'
        ? record.name
        : null;
  if (!packageName) {
    return null;
  }
  return {
    name: typeof record.name === 'string' ? record.name : undefined,
    package: packageName,
    // methods 为空表示“全启用导出的方法”（但仍会受 policy.allowedMethods 限制）。
    methods: Array.isArray(record.methods)
      ? record.methods.filter((item): item is string => typeof item === 'string')
      : [],
    signature: typeof record.signature === 'string' ? record.signature : undefined,
  };
}

// 动态插件模块允许 default export 或直接导出对象。
// 这里统一收敛为 `BridgePlugin` 形态，避免把模块系统差异扩散到 registry 主流程；
// 通过这个 helper 之后，`load()`/`resolveDynamicPlugins()` 只关心 bridge 协议要求的最小字段。
async function loadPluginModule(cwd: string, specifier: string): Promise<BridgePlugin> {
  const loaded = await resolveModuleFromCwd<unknown>(cwd, specifier);
  // 兼容 ESM default export 与直接导出对象两种写法，避免项目插件被模块格式绑死。
  const candidate =
    typeof loaded === 'object' &&
    loaded !== null &&
    'default' in (loaded as Record<string, unknown>)
      ? (loaded as { default: unknown }).default
      : loaded;
  if (!candidate || typeof candidate !== 'object') {
    throw new Error('plugin module must export an object');
  }
  const plugin = candidate as BridgePlugin;
  // registry 只校验最小 bridge 契约；更细的业务约束继续留给插件自身实现。
  if (
    typeof plugin.name !== 'string' ||
    !Array.isArray(plugin.methods) ||
    typeof plugin.handle !== 'function'
  ) {
    throw new Error('plugin export must include name/methods/handle');
  }
  return plugin;
}
