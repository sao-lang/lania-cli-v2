/**
 * toolkit-monorepo 模板的依赖清单。
 *
 * 说明：
 * - monorepo 的核心发布工具是 changesets，因此会默认加入 `@changesets/cli`。
 * - commitlint 这类“会落地 git hooks”的工具在本模板里倾向于把 husky/lint-staged 一并带上，
 *   减少用户后续手动补齐成本（与单包 toolkit 的策略略有不同）。
 * - useTs=true 时补齐 TS 与 ESLint TS 相关依赖（以及 runtime 的 tslib）。
 */
import { normalizeToolkitMonorepoOptions } from './options.js';

const TYPESCRIPT_DEV_DEPENDENCIES = ['typescript', '@types/node'];
const TYPESCRIPT_LINT_DEPENDENCIES = ['@typescript-eslint/eslint-plugin', '@typescript-eslint/parser'];
const BUILD_DEV_DEPENDENCIES = ['vite', 'vite-plugin-dts'];
const TEST_DEPENDENCIES = ['vitest'];
const RELEASE_DEV_DEPENDENCIES = ['@changesets/cli'];

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
    'husky',
    'lint-staged',
  ],
  editorconfig: ['editorconfig-checker'],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitMonorepoOptions(options);
  const devDependencies = new Set<string>();
  const dependencies = new Set<string>();

  if (normalized.useTs) {
    dependencies.add('tslib');
  }
  BUILD_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  RELEASE_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  normalized.lintTools.forEach((tool) => {
    (LINT_TOOL_DEPENDENCIES[tool] ?? []).forEach((item) => devDependencies.add(item));
  });
  if (normalized.useTs) {
    TYPESCRIPT_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
    TYPESCRIPT_LINT_DEPENDENCIES.forEach((item) => devDependencies.add(item));
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
  if (normalized.unitTestTool) {
    TEST_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }

  return {
    dependencies: Array.from(dependencies),
    devDependencies: Array.from(devDependencies),
  };
};
