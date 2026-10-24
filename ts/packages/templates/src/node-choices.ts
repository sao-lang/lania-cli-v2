import type { ChoiceSpec } from './language-options.js';

export const NODE_CHOICES: ChoiceSpec = {
  packageManagers: [
    { value: 'npm', label: 'npm' },
    { value: 'pnpm', label: 'pnpm' },
    { value: 'yarn', label: 'yarn' },
    { value: 'bun', label: 'bun' },
  ],
  defaultPackageManager: 'npm',
  lintTools: [
    { value: 'eslint', label: 'eslint' },
    { value: 'prettier', label: 'prettier' },
    { value: 'oxlint', label: 'oxlint' },
  ],
  defaultLintTools: ['eslint', 'prettier'],
  testTools: [
    { value: 'vitest', label: 'vitest' },
    { value: 'skip', label: 'skip' },
  ],
  defaultTestTool: 'vitest',
};
