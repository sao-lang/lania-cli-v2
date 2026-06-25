import type { WorkflowTransactionOptions } from '../types.js';

// Option readers for workflow step options.
// We keep them tiny and side-effect free so all step handlers can share the same parsing rules.

export function readLayers(options: Record<string, unknown> | undefined): string[] | undefined {
  return Array.isArray(options?.layers)
    ? options.layers.filter((item): item is string => typeof item === 'string' && item.length > 0)
    : undefined;
}

export function readContext(
  options: Record<string, unknown> | undefined,
): Record<string, unknown> | undefined {
  return options?.context && typeof options.context === 'object' && !Array.isArray(options.context)
    ? (options.context as Record<string, unknown>)
    : undefined;
}

export function readOptions(
  options: Record<string, unknown> | undefined,
): Record<string, unknown> | undefined {
  return options?.options && typeof options.options === 'object' && !Array.isArray(options.options)
    ? (options.options as Record<string, unknown>)
    : undefined;
}

export function readManager(options: Record<string, unknown> | undefined): string | undefined {
  return typeof options?.manager === 'string' && options.manager.length > 0
    ? options.manager
    : undefined;
}

export function readIncludeTemplateDependencies(
  options: Record<string, unknown> | undefined,
): boolean | undefined {
  return typeof options?.includeTemplateDependencies === 'boolean'
    ? options.includeTemplateDependencies
    : undefined;
}

export function readBooleanOption(
  options: Record<string, unknown> | undefined,
  key: string,
): boolean | undefined {
  return typeof options?.[key] === 'boolean' ? (options[key] as boolean) : undefined;
}

export function readActionNames(options: Record<string, unknown> | undefined): string[] | undefined {
  return Array.isArray(options?.actions)
    ? options.actions.filter((item): item is string => typeof item === 'string' && item.length > 0)
    : undefined;
}

export function readGuardNames(options: Record<string, unknown> | undefined): string[] | undefined {
  return Array.isArray(options?.guards)
    ? options.guards.filter((item): item is string => typeof item === 'string' && item.length > 0)
    : undefined;
}

export function shouldExecuteStep(options: Record<string, unknown> | undefined): boolean {
  // `dryRun` and `planOnly` are stronger signals than `execute`.
  if (readBooleanOption(options, 'dryRun') === true) {
    return false;
  }
  if (readBooleanOption(options, 'planOnly') === true) {
    return false;
  }
  return readBooleanOption(options, 'execute') !== false;
}

export function readTransactionOptions(options: unknown): WorkflowTransactionOptions | undefined {
  if (!options || typeof options !== 'object' || Array.isArray(options)) {
    return undefined;
  }
  const transaction = (options as Record<string, unknown>).transaction;
  return transaction && typeof transaction === 'object' && !Array.isArray(transaction)
    ? (transaction as WorkflowTransactionOptions)
    : undefined;
}

export function readTransactionLabel(options: unknown, fallback: string): string {
  const label = readTransactionOptions(options)?.label;
  return typeof label === 'string' && label.length > 0 ? label : fallback;
}

export function readTransactionTarget(options: unknown, fallback: string): string {
  const target = readTransactionOptions(options)?.target;
  return typeof target === 'string' && target.length > 0 ? target : fallback;
}

