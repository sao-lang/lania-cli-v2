export interface DartCliOptions {
  packageName: string;
}

export function normalizeDartCliOptions(
  options: Record<string, unknown> = {},
): DartCliOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName };
}
