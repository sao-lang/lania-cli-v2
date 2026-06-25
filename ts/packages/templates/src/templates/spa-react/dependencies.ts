/**
 * spa-react 模板的依赖清单。
 *
 * 依赖策略与 spa-vue 类似：
 * - `react/react-dom/@lania-tools/tools` 放在 runtime dependencies
 * - 构建器、lint、样式生态与类型依赖按 normalized options 动态补到 devDependencies
 * - 使用 Set 去重，避免同一依赖被多个选项分支重复加入
 */
import { normalizeSpaReactOptions } from './options.js';

const TYPESCRIPT_DEV_DEPENDENCIES = [
  '@types/react',
  '@types/react-dom',
  'typescript',
  '@types/node',
];

const WEBPACK_DEV_DEPENDENCIES = [
  'webpack-dev-server',
  '@babel/plugin-transform-runtime',
  '@babel/runtime',
  '@babel/preset-env',
  '@babel/core',
  'html-webpack-plugin',
  'mini-css-extract-plugin',
  'babel-loader',
  'copy-webpack-plugin',
  'css-loader',
  'css-minimizer-webpack-plugin',
  'style-loader',
  'postcss',
  'postcss-loader',
  'postcss-preset-env@10.3.1',
  '@pmmmwh/react-refresh-webpack-plugin',
  '@babel/preset-react',
  'webpack-bundle-analyzer',
  'react-refresh',
  'thread-loader',
  'terser-webpack-plugin',
];

const VITE_DEV_DEPENDENCIES = [
  '@vitejs/plugin-react',
  'vite-plugin-compression',
  'terser',
  'rollup-plugin-visualizer',
];

const LINT_TOOL_DEPENDENCIES: Record<string, string[]> = {
  eslint: [
    '@eslint/js@^9.39.0',
    'eslint-plugin-react',
    'eslint-plugin-react-hooks',
    'eslint@^9.39.0',
    'globals',
  ],
  oxlint: ['oxlint'],
  prettier: ['prettier'],
  oxfmt: ['oxfmt'],
  commitlint: [
    '@commitlint/cli',
    '@commitlint/config-conventional',
    'commitizen',
    'cz-customizable',
  ],
  stylelint: ['stylelint', 'stylelint-config-standard'],
  editorconfig: [],
};

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeSpaReactOptions(options);
  const devDependencies = new Set<string>([normalized.buildTool]);

  if (normalized.buildTool === 'webpack') {
    // webpack 需要 Babel / loader / HMR 相关生态；不同 cssProcessor 再额外补 loader。
    WEBPACK_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
    if (normalized.useTs) {
      devDependencies.add('@babel/preset-typescript');
    }
    switch (normalized.cssProcessor) {
      case 'less':
        devDependencies.add('less-loader');
        break;
      case 'sass':
        devDependencies.add('sass-loader');
        break;
      case 'stylus':
        devDependencies.add('stylus-loader');
        break;
      default:
        break;
    }
  } else {
    // vite 方案依赖相对更轻，主要是 react 插件与压缩/分析插件。
    VITE_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }

  normalized.lintTools.forEach((tool) => {
    (LINT_TOOL_DEPENDENCIES[tool] ?? []).forEach((item) => devDependencies.add(item));
  });

  if (normalized.lintTools.length > 0) {
    devDependencies.add('husky');
    devDependencies.add('lint-staged');
  }

  if (normalized.lintTools.includes('eslint') && normalized.useTs) {
    ['typescript-eslint', '@typescript-eslint/eslint-plugin', '@typescript-eslint/parser'].forEach(
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
  if (normalized.lintTools.includes('stylelint') && normalized.lintTools.includes('prettier')) {
    devDependencies.add('stylelint-prettier');
  }
  if (normalized.lintTools.includes('stylelint')) {
    if (normalized.cssProcessor === 'less') {
      ['postcss-less', 'stylelint-config-standard-less'].forEach((item) =>
        devDependencies.add(item),
      );
    }
    if (normalized.cssProcessor === 'sass') {
      ['stylelint-config-standard-scss'].forEach((item) => devDependencies.add(item));
    }
  }

  if (normalized.useTs) {
    TYPESCRIPT_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }

  if (normalized.cssProcessor !== 'css') {
    devDependencies.add(normalized.cssProcessor);
  }

  if (normalized.cssTools.includes('tailwindcss')) {
    ['tailwindcss', 'postcss', 'autoprefixer'].forEach((item) => devDependencies.add(item));
  }

  return {
    dependencies: ['react', 'react-dom', '@lania-tools/tools'],
    devDependencies: Array.from(devDependencies),
  };
};
