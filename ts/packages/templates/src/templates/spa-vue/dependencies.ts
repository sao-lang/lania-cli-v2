/**
 * spa-vue 模板的依赖清单。
 *
 * 依赖设计原则：
 * - dependencies：运行时依赖（用户代码真正 import 到的包）
 * - devDependencies：构建/脚手架/工程化依赖（只在开发/构建阶段使用）
 *
 * 这份清单由 options 决定：
 * - buildTool=vite/webpack 会影响 devDependencies（插件与 loader 不同）
 * - useTs 会补齐 typescript/vue-tsc 等类型相关依赖
 * - lintTools/cssTools 会按需补齐对应生态依赖，避免默认塞入无用包
 */
import { normalizeSpaVueOptions } from './options.js';

const TYPESCRIPT_DEV_DEPENDENCIES = ['vue-tsc', 'typescript', '@types/node'];

const WEBPACK_DEV_DEPENDENCIES = [
  'webpack-dev-server',
  '@babel/core',
  '@babel/preset-env',
  'babel-loader',
  'copy-webpack-plugin',
  'cross-env',
  'css-loader',
  'css-minimizer-webpack-plugin',
  'html-webpack-plugin',
  'mini-css-extract-plugin',
  'postcss-preset-env@10.3.1',
  'vue-loader',
  'vue-style-loader',
  'postcss-loader',
  'webpack-bundle-analyzer',
  '@vue/babel-plugin-jsx',
  'terser-webpack-plugin',
  'thread-loader',
];

const VITE_DEV_DEPENDENCIES = [
  '@vitejs/plugin-vue',
  'vite-plugin-compression',
  'vite-plugin-vue-setup-extend',
  'terser',
  'rollup-plugin-visualizer',
  '@vitejs/plugin-vue-jsx',
];

const LINT_TOOL_DEPENDENCIES: Record<string, string[]> = {
  eslint: [
    '@eslint/js@^9.39.0',
    'eslint-plugin-vue',
    'eslint@^9.39.0',
    'globals',
    'vue-eslint-parser',
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
  const normalized = normalizeSpaVueOptions(options);
  const devDependencies = new Set<string>([normalized.buildTool]);

  if (normalized.buildTool === 'webpack') {
    // webpack 方案需要额外的 loader/plugin 依赖；同时 cssProcessor 可能需要对应 loader。
    WEBPACK_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
    if (normalized.useTs) {
      ['ts-loader', '@babel/preset-typescript'].forEach((item) => devDependencies.add(item));
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
    // vite 方案主要依赖 @vitejs/plugin-vue 及若干常用生态插件。
    VITE_DEV_DEPENDENCIES.forEach((item) => devDependencies.add(item));
  }

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
  if (normalized.lintTools.includes('stylelint') && normalized.lintTools.includes('prettier')) {
    devDependencies.add('stylelint-prettier');
  }
  if (normalized.lintTools.includes('stylelint')) {
    if (normalized.cssProcessor === 'less') {
      ['postcss', 'postcss-less', 'stylelint-config-standard-less', 'stylelint-config-standard-vue', 'postcss-html'].forEach(
        (item) => devDependencies.add(item),
      );
    }
    if (normalized.cssProcessor === 'sass') {
      ['postcss', 'stylelint-config-standard-vue', 'stylelint-config-standard-scss', 'postcss-html'].forEach(
        (item) => devDependencies.add(item),
      );
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
    dependencies: ['vue', '@lania-tools/tools'],
    devDependencies: Array.from(devDependencies),
  };
};
