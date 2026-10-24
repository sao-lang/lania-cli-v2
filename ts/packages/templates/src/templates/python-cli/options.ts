export interface PythonCliOptions {
  packageName: string;
}

export function normalizePythonCliOptions(
  options: Record<string, unknown> = {},
): PythonCliOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName };
}
