import { normalizeChoices, type NormalizedChoices } from '../../language-options.js';
import { NODE_CHOICES } from '../../node-choices.js';

export interface ExpressOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeExpressOptions(
  options: Record<string, unknown> = {},
): ExpressOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'express-app';
  return { packageName, ...normalizeChoices(options, NODE_CHOICES) };
}
