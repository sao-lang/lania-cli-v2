import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';

export const RUST_CHOICES: ChoiceSpec = {
  packageManagers: [{ value: 'cargo', label: 'cargo' }],
  defaultPackageManager: 'cargo',
  lintTools: [
    { value: 'clippy', label: 'clippy' },
    { value: 'rustfmt', label: 'rustfmt' },
  ],
  defaultLintTools: ['clippy', 'rustfmt'],
  testTools: [
    { value: 'builtin', label: 'cargo test' },
    { value: 'skip', label: 'skip' },
  ],
  defaultTestTool: 'builtin',
};

export interface RustLibOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeRustLibOptions(
  options: Record<string, unknown> = {},
): RustLibOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName, ...normalizeChoices(options, RUST_CHOICES) };
}
