import { relative } from 'node:path';

// Small helpers shared by multiple product handlers.
// These are intentionally "dumb" utilities: no side effects, no IO.

export function firstNonEmptyString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim();
    }
  }
  return null;
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

// Path normalization used in reports to make output stable across platforms and machines.
export function toPortableRelativePath(from: string, to: string): string {
  const relativePath = relative(from, to);
  const normalized = relativePath.split('\\').join('/');
  return normalized.startsWith('.') ? normalized : `./${normalized}`;
}

export function structuredCloneRecord<T extends Record<string, unknown>>(value: T): T {
  // JSON round-tripping is sufficient here because our configs/reports are JSON-serializable.
  return JSON.parse(JSON.stringify(value)) as T;
}

export function normalizeGeneratedBinaryName(value: string): string {
  // We want a `bin` name that is:
  // - lowercase
  // - filesystem friendly
  // - stable
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/^@[^/]+\//, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .replace(/-cli$/, '') || 'product'
  );
}

export function resolveGeneratedPackageName(input: unknown, binaryName: string): string {
  const provided = firstNonEmptyString(input);
  return provided ?? `@lania-product/${binaryName}`;
}

export function escapeTemplateString(value: string): string {
  // Used when embedding user-provided strings into generated TS template strings.
  return value.replace(/\\/g, '\\\\').replace(/'/g, "\\'");
}

export function parseNonNegativeInteger(value: unknown, fallback: number): number {
  // Publish retry settings come from CLI params (string/number/undefined).
  if (value === undefined || value === null || value === '') {
    return fallback;
  }
  const parsed = Number.parseInt(String(value), 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error('publish retry settings must be non-negative integers');
  }
  return parsed;
}

export async function sleepMs(milliseconds: number): Promise<void> {
  if (milliseconds <= 0) {
    return;
  }
  await new Promise((resolve) => setTimeout(resolve, milliseconds));
}

