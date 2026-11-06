import type { BridgePlugin } from '../../core/bridge-plugin.js';
import {
  createDefaultPluginSecurityPolicy,
  intersectAllowedMethods,
  type RuntimePluginDeclaration,
  validatePluginDeclaration,
} from '../../core/plugin-policy.js';
import { resolveModuleFromCwd } from '../../core/runtime.js';

export async function loadCommandPlugin(
  cwd: string,
  declaration: RuntimePluginDeclaration,
  policy: ReturnType<typeof createDefaultPluginSecurityPolicy>,
  requestedMethod: string,
): Promise<BridgePlugin> {
  const validated = validatePluginDeclaration(cwd, declaration, policy);
  const loaded = await resolveModuleFromCwd<unknown>(cwd, validated.resolvedPath);
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
  if (
    typeof plugin.name !== 'string' ||
    !Array.isArray(plugin.methods) ||
    typeof plugin.handle !== 'function'
  ) {
    throw new Error('plugin export must include name/methods/handle');
  }

  const allowedMethods = intersectAllowedMethods(validated.methods, plugin.methods, policy);
  if (!allowedMethods.includes(requestedMethod)) {
    throw new Error(`plugin does not expose \`${requestedMethod}\` under current method policy`);
  }

  return {
    ...plugin,
    methods: allowedMethods,
  };
}

export function normalizePluginDeclarations(value: unknown): RuntimePluginDeclaration[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((entry) => normalizePluginDeclaration(entry))
    .filter((entry): entry is RuntimePluginDeclaration => entry !== null);
}

export function mergePluginDeclarations(...values: unknown[]): RuntimePluginDeclaration[] {
  const merged = new Map<string, RuntimePluginDeclaration>();
  for (const value of values) {
    for (const declaration of normalizePluginDeclarations(value)) {
      const key = `${declaration.package}::${declaration.name ?? ''}`;
      const current = merged.get(key);
      if (!current) {
        merged.set(key, declaration);
        continue;
      }
      merged.set(key, {
        ...current,
        ...declaration,
        methods: dedupeStrings([...(current.methods ?? []), ...(declaration.methods ?? [])]),
      });
    }
  }
  return [...merged.values()];
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

function dedupeStrings(values: string[]): string[] {
  return [...new Set(values)];
}
