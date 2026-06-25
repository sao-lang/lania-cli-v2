/**
 * 动态插件安全策略，负责声明校验、路径限制与方法白名单裁剪。
 *
 * 主要导出：createDefaultPluginSecurityPolicy、validatePluginDeclaration、intersectAllowedMethods、RuntimePluginDeclaration、PluginSecurityPolicy、ValidatedPluginDeclaration。
 *
 * 背景：
 * - bridge 会在 Node 进程内动态 import 项目插件（等同于执行用户代码）。
 * - 因此这里的策略目标是：在“可扩展”与“默认安全”之间取得平衡，让项目能用插件扩展能力，
 *   同时避免无意中执行不受信任的第三方包或越界的本地路径。
 *
 * 注意：
 * - 该文件只负责“声明级校验/裁剪”，真正的模块加载发生在 registry 层（resolveModuleFromCwd）。
 * - 签名字段目前只做“存在性要求”，不做加密验签；它更像是一个可插拔的约束开关。
 */
import { resolve } from 'node:path';

export interface RuntimePluginDeclaration {
  /** 展示名（可选）。registry 会用它覆盖插件模块自身导出的 name。 */
  name?: string;
  /** 包名或相对路径 specifier（例如 `@scope/pkg` / `./plugins/foo.ts`）。 */
  package: string;
  /**
   * 声明要启用的方法集合。
   * - 为空表示“全启用导出的方法”，但仍会受 policy.allowedMethods 限制。
   */
  methods?: string[];
  /**
   * 可选的“声明签名”字符串。
   * 当前只用于在 requireSignature=true 时作为存在性校验，避免 allowlist 外的包被随意加载。
   */
  signature?: string;
}

export interface PluginSecurityPolicy {
  /**
   * 显式允许的第三方插件包名列表。
   * - 当包名不满足 allowedPrefixes 时，只有落在 allowlist 内才允许加载。
   */
  allowlist: string[];
  /**
   * “一方/官方”插件包名前缀。
   * - 满足前缀的包默认视为更可信（trust=first_party）。
   */
  allowedPrefixes: string[];
  /**
   * 全局方法白名单。
   * - 为空表示不额外限制（仅以插件导出的方法为准）。
   * - 非空表示进一步裁剪插件暴露给 Rust 的 method 集合。
   */
  allowedMethods: string[];
  /**
   * 允许的插件来源：
   * - package: npm 包 specifier
   * - local_path: 项目内相对路径（./ 或 ../）
   */
  trustedSources: Array<'package' | 'local_path'>;
  /**
   * 是否要求“第三方包插件”必须带 signature。
   * - 注意：当前不做验签，只检查 declaration.signature 是否存在（或在 signatureAllowlist 中）。
   */
  requireSignature: boolean;
  /** 在 requireSignature=true 时允许跳过 signature 的包名列表。 */
  signatureAllowlist: string[];
}

export interface ValidatedPluginDeclaration extends RuntimePluginDeclaration {
  package: string;
  methods: string[];
  source: 'package' | 'local_path';
  /**
   * 解析后的加载入口：
   * - local_path: resolve(cwd, specifier)
   * - package: 原样 specifier（交由 resolveModuleFromCwd 从 cwd 的 node_modules 解析）
   */
  resolvedPath: string;
  /**
   * 信任等级（用于观测/诊断，并可作为未来更细粒度策略的扩展点）。
   * - first_party: 命中 allowedPrefixes
   * - allowlist: 命中 allowlist
   * - project_local: 本地相对路径插件
   */
  trust: 'first_party' | 'allowlist' | 'project_local';
}

export function createDefaultPluginSecurityPolicy(
  config: Record<string, unknown>,
): PluginSecurityPolicy {
  // 默认安全策略偏“保守”：
  // - 允许加载本地插件（相对路径）与 npm 包插件（specifier）
  // - npm 包插件必须满足前缀策略或 allowlist，避免随意执行第三方代码
  // - 可选要求签名，用于对 allowlist 外的包做额外校验
  return {
    allowlist: asStringArray(config.pluginAllowlist),
    allowedPrefixes: ['@lania/plugin-', 'lania-plugin-'],
    allowedMethods: asStringArray(config.pluginMethodAllowlist),
    trustedSources: asTrustedSources(config.pluginTrustedSources),
    requireSignature: config.pluginRequireSignature === true,
    signatureAllowlist: asStringArray(config.pluginSignatureAllowlist),
  };
}

export function validatePluginDeclaration(
  cwd: string,
  declaration: RuntimePluginDeclaration,
  policy: PluginSecurityPolicy,
): ValidatedPluginDeclaration {
  // 声明级校验（不执行模块）：
  // 1) 基本合法性（非空、无空白字符）
  // 2) 判定来源（local_path vs package），并检查 trustedSources
  // 3) local_path：限制在 workspace root 内，且限定扩展名
  // 4) package：检查 allowedPrefixes/allowlist，以及可选 signature 要求
  const specifier = declaration.package;
  if (!specifier || /\s/.test(specifier)) {
    throw new Error('plugin package name is invalid');
  }

  const source = isLocalPlugin(specifier) ? 'local_path' : 'package';
  const resolvedPath =
    source === 'local_path' ? resolve(cwd, specifier) : specifier;
  if (!policy.trustedSources.includes(source)) {
    throw new Error(`plugin source \`${source}\` is blocked by trust policy`);
  }

  if (source === 'local_path') {
    // 本地插件：要求在 workspace root 之内，且只允许常见 JS/TS 扩展名。
    // 这样可以避免通过 `../` 逃逸到工作区外执行任意文件。
    //
    // 限制说明：
    // - 这里用 startsWith(resolve(cwd)) 做字符串前缀判断，假设 resolve 后的路径是规范化的绝对路径。
    // - 若存在符号链接或大小写不一致的路径，严格的越界判断需要 realpath 级别的处理；
    //   当前实现以“足够好 + 轻量”为取舍，后续如需更强约束可升级。
    if (!resolvedPath.startsWith(resolve(cwd))) {
      throw new Error('local plugin must stay within workspace root');
    }
    if (!/\.(js|cjs|mjs|ts)$/.test(resolvedPath)) {
      throw new Error('local plugin path must end with .js/.cjs/.mjs/.ts');
    }
    return {
      ...declaration,
      methods: declaration.methods ?? [],
      source,
      resolvedPath,
      trust: 'project_local',
    };
  } else {
    // 包插件：默认只信任“官方前缀”或 allowlist。
    const allowedByPrefix = policy.allowedPrefixes.some((prefix) =>
      specifier.startsWith(prefix),
    );
    const allowedByWhitelist = policy.allowlist.includes(specifier);
    if (!allowedByPrefix && !allowedByWhitelist) {
      throw new Error(
        'third-party package plugin is blocked unless it matches prefix policy or allowlist',
      );
    }
    if (
      policy.requireSignature &&
      !allowedByPrefix &&
      !policy.signatureAllowlist.includes(specifier) &&
      !declaration.signature
    ) {
      throw new Error(
        'third-party package plugin requires signature or signature allowlist entry',
      );
    }
    return {
      ...declaration,
      methods: declaration.methods ?? [],
      source,
      resolvedPath,
      trust: allowedByPrefix ? 'first_party' : 'allowlist',
    };
  }
}

export function intersectAllowedMethods(
  declared: string[],
  exportedMethods: string[],
  policy: PluginSecurityPolicy,
): string[] {
  // 方法约束取交集：
  // - exportedMethods：插件真正导出的能力
  // - declared：用户声明想启用的能力（为空表示全启用）
  // - policy.allowedMethods：全局 allowlist（为空表示不额外限制）
  //
  // 这里的交集用于实现“最小权限”：
  // - 插件即使导出了更多 method，也不会被自动暴露
  // - 项目即使声明了 method，但插件没导出，也不会被伪造调用
  const allowlist =
    policy.allowedMethods.length > 0 ? policy.allowedMethods : exportedMethods;
  const base = declared.length > 0 ? declared : exportedMethods;
  // 注意：这里用 includes 做交集，复杂度 O(n^2) 但规模很小（methods 数量通常几十以内），
  // 换 Set 反而会让代码更噪；如果后续 methods 规模变大再优化。
  return base.filter(
    (method) => exportedMethods.includes(method) && allowlist.includes(method),
  );
}

function isLocalPlugin(specifier: string): boolean {
  // 本地插件只允许相对路径，避免用户误写成绝对路径或 file:// 导致绕过 workspace 限制。
  return specifier.startsWith('./') || specifier.startsWith('../');
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : [];
}

function asTrustedSources(value: unknown): Array<'package' | 'local_path'> {
  if (!Array.isArray(value)) {
    return ['package', 'local_path'];
  }
  const trustedSources = value.filter(
    (item): item is 'package' | 'local_path' =>
      item === 'package' || item === 'local_path',
  );
  return trustedSources.length > 0 ? trustedSources : ['package', 'local_path'];
}
