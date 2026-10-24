import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';

export const GO_CHOICES: ChoiceSpec = {
  packageManagers: [{ value: 'go', label: 'go modules' }],
  defaultPackageManager: 'go',
  lintTools: [
    { value: 'golangci-lint', label: 'golangci-lint' },
    { value: 'gofmt', label: 'gofmt' },
    { value: 'go-vet', label: 'go vet' },
  ],
  defaultLintTools: ['gofmt', 'go-vet'],
  testTools: [
    { value: 'go-test', label: 'go test' },
    { value: 'skip', label: 'skip' },
  ],
  defaultTestTool: 'go-test',
};

export interface GoServiceOptions extends NormalizedChoices {
  modulePath: string;
}

export function normalizeGoServiceOptions(
  options: Record<string, unknown> = {},
): GoServiceOptions {
  const modulePath =
    typeof options.modulePath === 'string' && options.modulePath.length > 0
      ? options.modulePath
      : 'example.com/demo';
  return { modulePath, ...normalizeChoices(options, GO_CHOICES) };
}
