import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';

export const DART_CHOICES: ChoiceSpec = {
  packageManagers: [{ value: 'pub', label: 'dart pub' }],
  defaultPackageManager: 'pub',
  lintTools: [{ value: 'dart-analyze', label: 'dart analyze' }],
  defaultLintTools: ['dart-analyze'],
  testTools: [
    { value: 'test', label: 'package:test' },
    { value: 'skip', label: 'skip' },
  ],
  defaultTestTool: 'test',
};

export interface DartCliOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeDartCliOptions(
  options: Record<string, unknown> = {},
): DartCliOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName, ...normalizeChoices(options, DART_CHOICES) };
}
