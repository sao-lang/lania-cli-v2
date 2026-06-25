/**
 * Hook 执行插件：安全地调度到项目插件的 hooks.invoke。
 *
 * 这个插件承担两类 hook 执行路径：
 * - `hooks.invoke`：调用“项目插件模块”里暴露的 hooks.invoke 能力
 * - `hooks.invokeInline`：调用当前 Node 进程内 registry 中保存的 inline hook 函数
 *
 * 安全边界：
 * - hook 名称必须在 `ALLOWED_HOOKS` allowlist 中
 * - 插件必须先在 `lan.config.plugins` 中声明，并通过 plugin-policy 校验
 * - inline hook 只接受字符串 id，真实函数始终留在 Node 内存中，不会被序列化给 Rust
 */
import type { BridgePlugin, BridgePluginResult } from '../core/bridge-plugin.js';
import type { BridgeEvent } from '../protocol/events.js';
import { invokeInlineHook } from '../core/inline-hooks.js';
import {
  createDefaultPluginSecurityPolicy,
  intersectAllowedMethods,
  validatePluginDeclaration,
  type RuntimePluginDeclaration,
} from '../core/plugin-policy.js';
import { asRecord, loadLanConfig, resolveModuleFromCwd } from '../core/runtime.js';

const ALLOWED_HOOKS = new Set([
  // v2.1 onXxx (public API)
  'onInitialize',
  'onCommandPreInit',
  'onArgsParsed',
  'onFilesPrepare',
  'onConfigGet',
  'onConfigResolve',
  'onFileWrite',
  'onTemplateParse',
  'onDependenciesModify',
  'onDependenciesInstall',
  'onInteractionPrompt',
  'onShellCommand',
  'onPluginApiCall',
  'onSuccess',
  'onError',
  'onPluginLoaded',
  'onCommandRegister',
  'onWorkflowStart',
  'onWorkflowComplete',
  'onShutdown',
]);

export const lifecyclePlugin: BridgePlugin = {
  name: 'lifecycle',
  methods: ['hooks.invoke', 'hooks.invokeInline'],
  async handle(method, params, context) {
    const cwd =
      (typeof params.cwd === 'string' ? params.cwd : context?.cwd) ?? process.cwd();

    switch (method) {
      case 'hooks.invoke':
        return invokeLifecycleHook(cwd, params);
      case 'hooks.invokeInline':
        return invokeInline(cwd, params);
      default:
        return null;
    }
  },
};

async function invokeLifecycleHook(
  cwd: string,
  params: Record<string, unknown>,
): Promise<BridgePluginResult<Record<string, unknown>>> {
  const hook = typeof params.hook === 'string' ? params.hook : '';
  const pluginRef = typeof params.plugin === 'string' ? params.plugin : '';
  const handler = typeof params.handler === 'string' ? params.handler : '';
  const kind = typeof params.kind === 'string' ? params.kind : 'parallel';
  if (!hook || !pluginRef || !handler) {
    throw new Error('hooks.invoke requires hook, plugin, and handler');
  }
  if (!ALLOWED_HOOKS.has(hook)) {
    // 显式 allowlist：防止项目插件通过 hooks.invoke 执行任意方法名，扩大攻击面。
    throw new Error(`hook \`${hook}\` is not allowed`);
  }

  const loaded = await loadLanConfig(cwd);
  const declarations = normalizePluginDeclarations(loaded.config.plugins);
  const declaration = declarations.find(
    (candidate) => candidate.package === pluginRef || candidate.name === pluginRef,
  );
  if (!declaration) {
    throw new Error(`plugin \`${pluginRef}\` is not declared in lan.config.plugins`);
  }

  const policy = createDefaultPluginSecurityPolicy(loaded.config);
  const plugin = await loadLifecyclePlugin(cwd, declaration, policy);
  // 对外始终调用 `hooks.invoke`：
  // 具体执行哪个 handler（如 onConfigResolve/onSuccess/...）由 payload 中的 `handler` 字段决定。
  const handled = await plugin.handle(
    'hooks.invoke',
    {
      hook,
      kind,
      handler,
      payload: params.payload ?? null,
      source: typeof params.source === 'string' ? params.source : 'host-runtime',
      cwd,
    },
    { cwd },
  );

  if (handled) {
    const normalized = asRecord(handled.result);
    const payload =
      kind === 'waterfall'
        // waterfall hook 的返回值约定：
        // - 优先取 { payload } 字段（与 v2 hook 约定一致）
        // - 否则退回到原始 result（兼容旧插件返回值形态）
        ? (Object.prototype.hasOwnProperty.call(normalized, 'payload') ? normalized.payload : handled.result)
        : undefined;
    return {
      result: {
        ok: true,
        accepted: true,
        hook,
        plugin: declaration.package,
        handler,
        ...(kind === 'waterfall' ? { payload } : {}),
      },
      events: handled.events,
    };
  }

  return {
    result: {
      ok: true,
      accepted: true,
      hook,
      plugin: declaration.package,
      handler,
    },
    events: [],
  };
}

async function invokeInline(
  cwd: string,
  params: Record<string, unknown>,
): Promise<BridgePluginResult<Record<string, unknown>>> {
  const hook = typeof params.hook === 'string' ? params.hook : null;
  if (!hook || !ALLOWED_HOOKS.has(hook)) {
    return {
      result: {
        ok: false,
        error: `unsupported hook: ${hook ?? 'unknown'}`,
      },
      events: [],
    };
  }
  const source = typeof params.source === 'string' ? params.source : 'host-runtime';
  const kind = typeof params.kind === 'string' ? params.kind : 'parallel';
  const inlineId = typeof params.id === 'string' ? params.id : null;
  const payload = params.payload ?? {};
  if (!inlineId) {
    return {
      result: { ok: false, error: 'inline id is required' },
      events: [],
    };
  }
  const handled = await invokeInlineHook(cwd, inlineId, payload, {
    cwd,
    hook,
    kind,
    source,
  });
  // inline hook 的实现完全在 node 侧（内存里），这里仅负责把 payload/events 按协议回传给 Rust。
  return {
    result: {
      ok: true,
      payload: handled.payload,
    },
    events: handled.events,
  };
}

async function loadLifecyclePlugin(
  cwd: string,
  declaration: RuntimePluginDeclaration,
  policy: ReturnType<typeof createDefaultPluginSecurityPolicy>,
): Promise<BridgePlugin> {
  // 这里与 registry.loadPluginModule 类似，但额外要求 methods 中必须保留 `hooks.invoke`。
  const validated = validatePluginDeclaration(cwd, declaration, policy);
  const loaded = await resolveModuleFromCwd<unknown>(cwd, validated.resolvedPath);
  const candidate =
    typeof loaded === 'object' && loaded !== null && 'default' in (loaded as Record<string, unknown>)
      ? (loaded as { default: unknown }).default
      : loaded;
  if (!candidate || typeof candidate !== 'object') {
    throw new Error('plugin module must export an object');
  }
  const plugin = candidate as BridgePlugin;
  if (typeof plugin.name !== 'string' || !Array.isArray(plugin.methods) || typeof plugin.handle !== 'function') {
    throw new Error('plugin export must include name/methods/handle');
  }

  const allowedMethods = intersectAllowedMethods(
    validated.methods,
    plugin.methods,
    policy,
  );
  if (!allowedMethods.includes('hooks.invoke')) {
    throw new Error('plugin does not expose hooks.invoke under current method policy');
  }

  return {
    ...plugin,
    methods: allowedMethods,
  };
}

function normalizePluginDeclarations(value: unknown): RuntimePluginDeclaration[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((entry) => normalizePluginDeclaration(entry))
    .filter((entry): entry is RuntimePluginDeclaration => entry !== null);
}

function normalizePluginDeclaration(value: unknown): RuntimePluginDeclaration | null {
  if (typeof value === 'string') {
    return { package: value };
  }
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const record = value as Record<string, unknown>;
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
    methods: Array.isArray(record.methods)
      ? record.methods.filter((item): item is string => typeof item === 'string')
      : [],
    signature: typeof record.signature === 'string' ? record.signature : undefined,
  };
}
