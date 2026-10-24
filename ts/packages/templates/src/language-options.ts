export interface ChoiceOption {
  value: string;
  label: string;
}

export interface ChoiceSpec {
  packageManagers: ChoiceOption[];
  defaultPackageManager: string;
  lintTools: ChoiceOption[];
  defaultLintTools: string[];
  testTools: ChoiceOption[];
  defaultTestTool: string;
}

export interface NormalizedChoices {
  packageManager: string;
  lintTools: string[];
  testTool: string;
}

export interface ChoiceQuestion {
  name: string;
  type: string;
  message: string;
  choices?: string[];
  default?: unknown;
}

export function normalizeChoices(
  options: Record<string, unknown>,
  spec: ChoiceSpec,
): NormalizedChoices {
  const packageManagerValues = spec.packageManagers.map((option) => option.value);
  const lintValues = spec.lintTools.map((option) => option.value);
  const testValues = spec.testTools.map((option) => option.value);

  const packageManager =
    typeof options.packageManager === 'string' &&
    packageManagerValues.includes(options.packageManager)
      ? options.packageManager
      : spec.defaultPackageManager;

  const lintTools = Array.isArray(options.lintTools)
    ? options.lintTools.filter(
        (value): value is string => typeof value === 'string' && lintValues.includes(value),
      )
    : spec.defaultLintTools;

  const testTool =
    typeof options.testTool === 'string' && testValues.includes(options.testTool)
      ? options.testTool
      : spec.defaultTestTool;

  return { packageManager, lintTools, testTool };
}

export function buildChoiceQuestions(
  spec: ChoiceSpec,
  options: Record<string, unknown>,
  zh: boolean,
): ChoiceQuestion[] {
  const normalized = normalizeChoices(options, spec);
  const questions: ChoiceQuestion[] = [];
  if (spec.packageManagers.length > 1) {
    questions.push({
      name: 'packageManager',
      type: 'list',
      message: zh ? '请选择包管理器：' : 'Package manager:',
      choices: spec.packageManagers.map((option) => option.value),
      default: normalized.packageManager,
    });
  }
  questions.push({
    name: 'lintTools',
    type: 'checkbox',
    message: zh ? '请选择 lint 工具：' : 'Lint tools:',
    choices: spec.lintTools.map((option) => option.value),
    default: normalized.lintTools,
  });
  questions.push({
    name: 'testTool',
    type: 'list',
    message: zh ? '请选择测试工具：' : 'Test tool:',
    choices: spec.testTools.map((option) => option.value),
    default: normalized.testTool,
  });
  return questions;
}
