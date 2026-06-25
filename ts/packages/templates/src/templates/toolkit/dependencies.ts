/**
 * toolkit 模板的依赖清单。
 *
 * 依赖策略：
 * - runtime 依赖尽量少，只有在 useTs=true 时补 `tslib`
 * - 其余构建/测试/发布/工程化能力都放到 devDependencies
 * - 通过 Set 去重，避免多个选项分支重复加入同一依赖
 */
import { normalizeToolkitOptions } from './options.js';

const TYPESCRIPT_DEV_DEPENDENCIES = ['typescript', '@types/node'];
const TYPESCRIPT_LINT_DEPENDENCIES = ['@typescript-eslint/eslint-plugin', '@typescript-eslint/parser'];
const BUILD_DEV_DEPENDENCIES = ['vite', 'vite-plugin-dts'];
const TEST_DEPENDENCIES = ['vitest'];
const RELEASE_DEV_DEPENDENCIES = ['conventional-changelog-cli'];

const LINT_TOOL_DEPENDENCIES: Record<string, string[]> = {
  eslint: ['@eslint/js@^9.39.0', 'eslint@^9.39.0', 'globals'],
  oxlint: ['oxlint'],
  prettier: ['prettier'],
  oxfmt: ['oxfmt'],
  commitlint: [
    '@commitlint/cli',
    '@commitlint/config-conventional',
    'commitizen',
    'cz-customizable',
  ],
  editorconfig: [],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitOptions(options);
  const devDependencies = new Set<string>();
  const dependencies = new Set<string>();

  if (normalized.useTs) {
    // 工具库在 TS 输出到 JS 时通常需要 tslib 承载 helper。
    dependencies.add('tslib');
  }
  BUILD_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  normalized.lintTools.forEach((tool) => {
    (LINT_TOOL_DEPENDENCIES[tool] ?? []).forEach((item) => devDependencies.add(item));
  });
  if (normalized.lintTools.length > 0) {
    ['husky', 'lint-staged'].forEach((item) => devDependencies.add(item));
  }
  if (normalized.lintTools.includes('eslint') && normalized.useTs) {
    ['typescript-eslint', '@typescript-eslint/parser', '@typescript-eslint/eslint-plugin'].forEach(
      (item) => devDependencies.add(item),
    );
  }
  if (normalized.lintTools.includes('eslint') && normalized.lintTools.includes('oxlint')) {
    devDependencies.add('eslint-plugin-oxlint');
  }
  // 只有在 eslint + prettier 且未启用 oxfmt 时，才把 prettier 集成到 eslint 生态里。
  if (
    normalized.lintTools.includes('eslint') &&
    normalized.lintTools.includes('prettier') &&
    !normalized.lintTools.includes('oxfmt')
  ) {
    ['eslint-config-prettier', 'eslint-plugin-prettier'].forEach((item) =>
      devDependencies.add(item),
    );
  }
  if (normalized.useTs) {
    TYPESCRIPT_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
    TYPESCRIPT_LINT_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }
  if (normalized.unitTestTool) {
    TEST_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }
  RELEASE_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));

  return {
    dependencies: Array.from(dependencies),
    devDependencies: Array.from(devDependencies),
  };
};
