import { normalizeDartCliOptions } from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeDartCliOptions(options);
  const devDependencies: string[] = [];
  if (normalized.lintTools.includes('dart-analyze')) {
    devDependencies.push('lints');
  }
  if (normalized.testTool === 'test') {
    devDependencies.push('test');
  }
  return {
    dependencies: [] as string[],
    devDependencies,
  };
};
