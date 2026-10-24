export interface RustLibOptions {
  packageName: string;
}

export function normalizeRustLibOptions(
  options: Record<string, unknown> = {},
): RustLibOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName };
}
