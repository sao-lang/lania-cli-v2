import { normalizePythonCliOptions } from './options.js';

const LINT_DEPENDENCIES: Record<string, string[]> = {
  ruff: ['ruff'],
  black: ['black'],
  mypy: ['mypy'],
  flake8: ['flake8'],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizePythonCliOptions(options);
  const devDependencies = new Set<string>();
  normalized.lintTools.forEach((tool) => {
    (LINT_DEPENDENCIES[tool] ?? []).forEach((dependency) => devDependencies.add(dependency));
  });
  if (normalized.testTool === 'pytest') {
    devDependencies.add('pytest');
  }
  return {
    dependencies: [] as string[],
    devDependencies: Array.from(devDependencies),
  };
};
