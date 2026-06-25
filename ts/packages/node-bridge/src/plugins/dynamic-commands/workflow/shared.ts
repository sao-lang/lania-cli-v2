// Small pure helpers shared across workflow step execution modules.

export function joinOrNone(values: string[]): string {
  return values.length > 0 ? values.join(', ') : 'none';
}

export function uniqueStrings(values: string[]): string[] {
  const result: string[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value || seen.has(value)) {
      continue;
    }
    seen.add(value);
    result.push(value);
  }
  return result;
}

export function assertNever(value: never): never {
  throw new Error(`unsupported workflow step: ${String(value)}`);
}

