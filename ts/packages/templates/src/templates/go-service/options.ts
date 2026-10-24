export interface GoServiceOptions {
  modulePath: string;
}

export function normalizeGoServiceOptions(
  options: Record<string, unknown> = {},
): GoServiceOptions {
  const modulePath =
    typeof options.modulePath === 'string' && options.modulePath.length > 0
      ? options.modulePath
      : 'example.com/demo';
  return { modulePath };
}
