import { asRecord } from '../../core/runtime.js';
import type { CommandPackMeta, GeneratedCommandSpec } from './types.js';

export function parseCommandPackMeta(value: unknown): CommandPackMeta | undefined {
  const record = asRecord(value);
  if (Object.keys(record).length === 0) {
    return undefined;
  }
  return {
    about: typeof record.about === 'string' ? record.about : undefined,
    alias: typeof record.alias === 'string' ? record.alias : undefined,
    aliases: stringArray(record.aliases, []),
    examples: parseExamples(record.examples),
  };
}

export function parseExamples(value: unknown): GeneratedCommandSpec['examples'] {
  return Array.isArray(value)
    ? value
        .map((item) => {
          const example = asRecord(item);
          return {
            command: typeof example.command === 'string' ? example.command : '',
            description: typeof example.description === 'string' ? example.description : '',
          };
        })
        .filter((example) => example.command)
    : [];
}

export function normalizeValueKind(
  value: unknown,
): GeneratedCommandSpec['options'][number]['value_kind'] {
  return value === 'bool' || value === 'string' || value === 'number' || value === 'optional_string'
    ? value
    : 'string';
}

export function applyCommandPackMeta(
  root: GeneratedCommandSpec,
  meta: CommandPackMeta | undefined,
) {
  if (!meta) {
    return;
  }
  if (meta.about) {
    root.about = meta.about;
  }
  if (meta.alias) {
    root.alias = meta.alias;
  }
  if (meta.aliases && meta.aliases.length > 0) {
    root.aliases = [...new Set([...root.aliases, ...meta.aliases])];
  }
  if (meta.examples && meta.examples.length > 0) {
    root.examples = [...meta.examples];
  }
}

export function createCommandSpec(
  name: string,
  about: string,
  handlerId: string,
): GeneratedCommandSpec {
  return {
    name,
    about,
    alias: null,
    aliases: [],
    args: [],
    options: [],
    examples: [],
    subcommands: [],
    handler_id: handlerId,
  };
}

export function stringArray(value: unknown, fallback: string[]): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : fallback;
}

export function sanitizeSegment(value: string): string {
  return value
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/[^a-zA-Z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-{2,}/g, '-')
    .toLowerCase();
}
