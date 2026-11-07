type DynamicRuntimeMode = 'development' | 'installed';

export interface DynamicRuntimeRoots {
  mode: DynamicRuntimeMode;
  invocationCwd: string;
  workspaceRoot: string;
  productRoot: string;
}

export function resolveDynamicRuntimeRoots(
  cwd: string,
  params?: Record<string, unknown>,
): DynamicRuntimeRoots {
  const invocationCwd = cwd;
  const workspaceRoot =
    readString(params?.workspaceRoot) ?? readEnvPath('LANIA_WORKSPACE_ROOT') ?? cwd;
  const productRoot =
    readString(params?.productRoot) ?? readEnvPath('LANIA_PRODUCT_ROOT') ?? workspaceRoot;
  const explicitMode = normalizeRuntimeMode(
    readString(params?.runtimeMode) ?? process.env.LANIA_RUNTIME_MODE ?? null,
  );

  return {
    mode: explicitMode ?? (productRoot !== workspaceRoot ? 'installed' : 'development'),
    invocationCwd,
    workspaceRoot,
    productRoot,
  };
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null;
}

function readEnvPath(name: string): string | null {
  return readString(process.env[name]);
}

function normalizeRuntimeMode(value: string | null): DynamicRuntimeMode | null {
  if (value === 'development' || value === 'installed') {
    return value;
  }
  return null;
}
