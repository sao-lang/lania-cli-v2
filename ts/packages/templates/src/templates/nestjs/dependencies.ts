import { normalizeNestjsOptions } from './options.js';

const LINT_DEPENDENCIES: Record<string, string[]> = {
  eslint: ['eslint', '@eslint/js', 'typescript-eslint'],
  prettier: ['prettier', 'eslint-config-prettier'],
  oxlint: ['oxlint'],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeNestjsOptions(options);
  const devDependencies = new Set<string>([
    'typescript',
    '@types/node',
    '@nestjs/cli',
    'ts-node',
    'tsconfig-paths',
  ]);
  normalized.lintTools.forEach((tool) => {
    (LINT_DEPENDENCIES[tool] ?? []).forEach((dependency) => devDependencies.add(dependency));
  });
  if (normalized.testTool === 'vitest') {
    devDependencies.add('vitest');
  }
  return {
    dependencies: [
      '@nestjs/common',
      '@nestjs/core',
      '@nestjs/platform-express',
      'reflect-metadata',
      'rxjs',
    ],
    devDependencies: Array.from(devDependencies),
  };
};
