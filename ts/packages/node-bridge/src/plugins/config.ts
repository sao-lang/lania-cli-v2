/**
 * 配置插件，向 Rust 暴露 lan.config 与工具配置读取能力。
 *
 * 主要导出：configPlugin。
 *
 * 主要职责：
 * - `config.loadLan`：读取并返回 `lan.config.*`，同时把其中的函数型 hooks 转成可序列化的 inline hook 引用
 * - `config.loadTool`：读取 vite/webpack/eslint/prettier/stylelint/commitlint/commitizen 等工具配置
 *
 * 为什么要 sanitize hooks：
 * - Rust 无法持有 JS 函数，因此配置里的函数型 hook 不能原样跨进程序列化
 * - 这里会把函数注册进 Node 侧 inline-hooks registry，并把配置内容替换成 `{ type:'inline', id }`
 */
import type { BridgeEvent } from '../protocol/events.js';
import { loadLanConfig, loadToolConfig } from '../core/runtime.js';
import { registerInlineHook } from '../core/inline-hooks.js';

const SUPPORTED_EXTENSIONS = ['.js', '.cjs', '.json', '.ts'];

export const configPlugin = {
  name: 'config',
  methods: ['config.loadLan', 'config.loadTool'],
  async handle(method: string, params: Record<string, unknown>) {
    const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();

    switch (method) {
      case 'config.loadLan': {
        const candidates = Array.isArray(params.candidates)
          ? params.candidates.filter((item): item is string => typeof item === 'string')
          : undefined;
        const searchFrom = typeof params.searchFrom === 'string' ? params.searchFrom : undefined;
        const loaded = await loadLanConfig(cwd, { searchFrom, candidates });
        const sanitizedConfig = sanitizeConfigHooks(cwd, loaded.config);
        return {
          result: {
            cwd,
            configPath: loaded.configPath,
            exists: loaded.exists,
            supportedExtensions: SUPPORTED_EXTENSIONS,
            config: sanitizedConfig,
            buildTool: (sanitizedConfig as any).buildTool ?? 'vite',
            buildAdaptors: (sanitizedConfig as any).buildAdaptors ?? {},
            lintAdaptors: (sanitizedConfig as any).lintAdaptors ?? {},
            lintTools: (sanitizedConfig as any).lintTools ?? [],
          },
          events: [
            {
              method: 'event.log',
              params: {
                level: 'info',
                message: loaded.exists
                  ? `Loaded lan config from ${loaded.configPath}`
                  : 'No lan config found, using defaults',
              },
            } satisfies BridgeEvent,
          ],
        };
      }
      case 'config.loadTool': {
        const tool = typeof params.tool === 'string' ? params.tool : 'unknown';
        const loaded = await loadToolConfig(cwd, tool);
        return {
          result: {
            cwd,
            tool,
            configPath: loaded.configPath,
            exists: loaded.exists,
            config: loaded.config,
            resolved: loaded.exists,
          },
          events: [] as BridgeEvent[],
        };
      }
      default:
        return null;
    }
  },
};

function sanitizeConfigHooks(cwd: string, config: any): any {
  if (!config || typeof config !== 'object') return config;
  const hooks = (config as any).hooks;
  if (!hooks || typeof hooks !== 'object') return config;

  const cloned = { ...(config as any) };
  const hooksRecord = hooks as Record<string, unknown>;
  const sanitizedHooks: Record<string, unknown> = {};

  for (const [rawKey, value] of Object.entries(hooksRecord)) {
    const hookKey =
      rawKey === 'preRun' ? 'onCommandPreInit' : rawKey === 'postRun' ? 'onSuccess' : rawKey;
    // 兼容旧命名：preRun/postRun 在 bridge 层统一映射到新的 onXxx 命名。
    if (!Array.isArray(value)) {
      sanitizedHooks[hookKey] = value;
      continue;
    }
    const items: unknown[] = [];
    for (let index = 0; index < value.length; index++) {
      const entry = value[index];
      if (typeof entry === 'function') {
        const id = `inline:lan.config:${hookKey}:${index}`;
        registerInlineHook(cwd, id, entry as any);
        // 函数型 hook 只保留元信息；Rust 侧后续通过 hooks.invokeInline 把 id 再回调给 Node 执行。
        items.push({
          type: 'inline',
          id,
          kind: hookKey === 'onCommandPreInit' ? 'waterfall' : 'parallel',
        });
      } else {
        items.push(entry);
      }
    }
    sanitizedHooks[hookKey] = items;
  }

  cloned.hooks = sanitizedHooks;
  return cloned;
}
