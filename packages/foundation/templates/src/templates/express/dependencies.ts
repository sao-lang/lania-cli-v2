import { normalizeExpressOptions } from './options.js';

const LINT_DEPENDENCIES: Record<string, string[]> = {
  eslint: ['eslint', '@eslint/js', 'typescript-eslint'],
  prettier: ['prettier', 'eslint-config-prettier'],
  oxlint: ['oxlint'],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeExpressOptions(options);
  const devDependencies = new Set<string>([
    'typescript',
    'tsx',
    '@types/node',
    '@types/express',
  ]);
  normalized.lintTools.forEach((tool) => {
    (LINT_DEPENDENCIES[tool] ?? []).forEach((dependency) => devDependencies.add(dependency));
  });
  if (normalized.testTool === 'vitest') {
    devDependencies.add('vitest');
  }
  return {
    dependencies: ['express'],
    devDependencies: Array.from(devDependencies),
  };
};
