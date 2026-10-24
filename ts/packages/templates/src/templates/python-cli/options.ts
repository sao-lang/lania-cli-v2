import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';

export const PYTHON_CHOICES: ChoiceSpec = {
  packageManagers: [
    { value: 'pip', label: 'pip' },
    { value: 'uv', label: 'uv' },
    { value: 'poetry', label: 'poetry' },
  ],
  defaultPackageManager: 'pip',
  lintTools: [
    { value: 'ruff', label: 'ruff' },
    { value: 'black', label: 'black' },
    { value: 'mypy', label: 'mypy' },
    { value: 'flake8', label: 'flake8' },
  ],
  defaultLintTools: ['ruff'],
  testTools: [
    { value: 'pytest', label: 'pytest' },
    { value: 'skip', label: 'skip' },
  ],
  defaultTestTool: 'pytest',
};

export interface PythonCliOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizePythonCliOptions(
  options: Record<string, unknown> = {},
): PythonCliOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName, ...normalizeChoices(options, PYTHON_CHOICES) };
}
