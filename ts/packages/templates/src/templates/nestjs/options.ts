import { normalizeChoices, type NormalizedChoices } from '../../language-options.js';
import { NODE_CHOICES } from '../../node-choices.js';

export interface NestjsOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeNestjsOptions(
  options: Record<string, unknown> = {},
): NestjsOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'nestjs-app';
  return { packageName, ...normalizeChoices(options, NODE_CHOICES) };
}
