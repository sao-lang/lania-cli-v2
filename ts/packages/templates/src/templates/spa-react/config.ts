/**
 * spa-react 模板的输出文件规则与渲染配置。
 *
 * 生成规则与 spa-vue 基本一致，只是业务骨架与构建插件替换为 React 生态版本。
 * files 列表按以下维度裁剪：
 * - buildTool：vite / webpack
 * - useTs：TSX / JSX、tsconfig、vite-env 等
 * - cssProcessor / cssTools：样式文件、tailwind/postcss 配置
 * - lintTools：工程化配置与 husky 脚本
 */
import { normalizeSpaReactOptions } from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeSpaReactOptions(options);
  const hasLintTool = (tool: string) => normalized.lintTools.includes(tool);
  // 只要启用了任一会落地 git hook 的工具，就补齐 husky 相关文件。
  const hasAnyGitHookTool = ['eslint', 'oxlint', 'prettier', 'oxfmt', 'stylelint', 'commitlint'].some((tool) =>
    normalized.lintTools.includes(tool),
  );
  const files = [
    // 构建入口与基础运行时文件：根据 buildTool / useTs / cssProcessor 挑选一组互斥模板。
    file('webpack.config.cjs', 'ejs/webpack.config.cjs.ejs', normalized.buildTool === 'webpack'),
    file(
      'vite.config.ts',
      'ejs/vite.config.ts.ejs',
      normalized.buildTool === 'vite' && normalized.useTs,
    ),
    file(
      'vite.config.js',
      'ejs/vite.config.js.ejs',
      normalized.buildTool === 'vite' && !normalized.useTs,
    ),
    file('vite-env.d.ts', 'ejs/vite-env.d.ts.ejs', normalized.buildTool === 'vite' && normalized.useTs),
    file('tsconfig.json', 'ejs/tsconfig.json.ejs', normalized.useTs),
    file('tailwind.css', 'ejs/tailwind.css.ejs', normalized.cssTools.includes('tailwindcss')),
    file(
      'tailwind.config.cjs',
      'ejs/tailwind.config.cjs.ejs',
      normalized.cssTools.includes('tailwindcss'),
    ),
    file(
      'postcss.config.cjs',
      'ejs/postcss.config.cjs.ejs',
      normalized.cssTools.includes('tailwindcss'),
    ),
    file('package.json', 'ejs/package.json.ejs'),
    file('src/main.tsx', 'ejs/main.tsx.ejs', normalized.useTs),
    file('src/main.jsx', 'ejs/main.jsx.ejs', !normalized.useTs),
    file('src/utils/request/index.ts', 'ejs/request-index.ts.ejs', normalized.useTs),
    file('src/utils/request/index.js', 'ejs/request-index.js.ejs', !normalized.useTs),
    file('src/utils/request/config.ts', 'ejs/request-config.ts.ejs', normalized.useTs),
    file('src/utils/request/config.js', 'ejs/request-config.js.ejs', !normalized.useTs),
    file('src/api/modules/system.ts', 'ejs/system-api.ts.ejs', normalized.useTs),
    file('src/api/modules/system.js', 'ejs/system-api.js.ejs', !normalized.useTs),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('src/index.styl', 'ejs/index.styl.ejs', normalized.cssProcessor === 'stylus'),
    file('src/index.scss', 'ejs/index.scss.ejs', normalized.cssProcessor === 'sass'),
    file('src/index.less', 'ejs/index.less.ejs', normalized.cssProcessor === 'less'),
    file('index.html', 'ejs/index.html.ejs'),
    file('src/index.css', 'ejs/index.css.ejs', normalized.cssProcessor === 'css'),
    file('src/App.tsx', 'ejs/App.tsx.ejs', normalized.useTs),
    file('src/App.styl', 'ejs/App.styl.ejs', normalized.cssProcessor === 'stylus'),
    file('src/App.scss', 'ejs/App.scss.ejs', normalized.cssProcessor === 'sass'),
    file('src/App.less', 'ejs/App.less.ejs', normalized.cssProcessor === 'less'),
    file('src/App.jsx', 'ejs/App.jsx.ejs', !normalized.useTs),
    file('src/App.css', 'ejs/App.css.ejs', normalized.cssProcessor === 'css'),
    // 工程化配置按需开启，避免给最小模板额外塞入无用文件。
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('eslint.config.js', 'ejs/eslint.config.js.ejs', hasLintTool('eslint')),
    file('.oxlintrc.json', 'ejs/.oxlintrc.json.ejs', hasLintTool('oxlint')),
    file('stylelint.config.cjs', 'ejs/stylelint.config.cjs.ejs', hasLintTool('stylelint')),
    file('prettier.config.cjs', 'ejs/prettier.config.cjs.ejs', hasLintTool('prettier')),
    file('.oxfmtrc.json', 'ejs/.oxfmtrc.json.ejs', hasLintTool('oxfmt')),
    file('commitlint.config.cjs', 'ejs/commitlint.config.cjs.ejs', hasLintTool('commitlint')),
    file('.stylelintignore', 'ejs/.stylelintignore.ejs', hasLintTool('stylelint')),
    file('.prettierignore', 'ejs/.prettierignore.ejs', hasLintTool('prettier')),
    file('.editorconfig', 'ejs/.editorconfig.ejs', hasLintTool('editorconfig')),
    file('.czrc.cjs', 'ejs/.czrc.cjs.ejs', hasLintTool('commitlint')),
    file('.husky/commit-msg', 'ejs/commit-msg.ejs', hasAnyGitHookTool),
    file('.husky/pre-commit', 'ejs/pre-commit.ejs', hasAnyGitHookTool),
  ].filter((item): item is { outputPath: string; templatePath: string } => Boolean(item));

  return {
    outputTasks: ['write-files', 'install-deps'],
    files,
  };
};

function file(outputPath: string, templatePath: string, enabled = true) {
  return enabled ? { outputPath, templatePath } : null;
}
