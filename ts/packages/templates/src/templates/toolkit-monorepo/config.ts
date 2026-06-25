/**
 * toolkit-monorepo 模板的输出文件规则与渲染配置。
 *
 * 目标结构：
 * - workspace 根：package.json、lan.config.js、changesets、husky、共享 tsconfig/eslint/prettier 等
 * - `packages/core`：示例子包（入口、vite 配置、可选 tsconfig）
 *
 * 与单包 toolkit 的区别：
 * - 这里会同时生成根配置与子包模板，因此 files 列表更像“一个最小 monorepo 脚手架”。
 */
import { normalizeToolkitMonorepoOptions } from './options.js';

function file(outputPath: string, templatePath: string, include = true) {
  // `include` 会在后续模板运行时被当作布尔开关使用；
  // 这里保留字段而不是直接返回 null，是为了让调用方可保留“文件规则对象”这一形态。
  return { outputPath, templatePath, include };
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitMonorepoOptions(options);
  const hasLintTool = (tool: string) => normalized.lintTools.includes(tool);
  // monorepo 下也只在需要时生成 git hooks，避免给最小模板增加额外流程。
  const hasAnyGitHookTool = ['eslint', 'oxlint', 'prettier', 'oxfmt', 'stylelint', 'commitlint'].some((tool) =>
    normalized.lintTools.includes(tool),
  );

  return {
    files: [
      // workspace 根级文件：包管理、发布、共享配置与脚本。
      file('README.md', 'files/README.md.ejs'),
      file('package.json', 'ejs/package.json.ejs'),
      file('lan.config.js', 'ejs/lan.config.js.ejs'),
      file('scripts/run-package.mjs', 'ejs/scripts/run-package.mjs.ejs'),
      file('pnpm-workspace.yaml', 'ejs/pnpm-workspace.yaml.ejs', normalized.packageManager === 'pnpm'),
      file('tsconfig.base.json', 'ejs/tsconfig.base.json.ejs', normalized.useTs),
      file('eslint.config.js', 'ejs/eslint.config.js.ejs', hasLintTool('eslint')),
      file('.oxlintrc.json', 'ejs/.oxlintrc.json.ejs', hasLintTool('oxlint')),
      file('prettier.config.cjs', 'ejs/prettier.config.cjs.ejs', hasLintTool('prettier')),
      file('.prettierignore', 'ejs/.prettierignore.ejs', hasLintTool('prettier')),
      file('.oxfmtrc.json', 'ejs/.oxfmtrc.json.ejs', hasLintTool('oxfmt')),
      file('commitlint.config.cjs', 'ejs/commitlint.config.cjs.ejs', hasLintTool('commitlint')),
      file('.editorconfig', 'ejs/.editorconfig.ejs', hasLintTool('editorconfig')),
      file('.gitignore', 'ejs/.gitignore.ejs'),
      file('.husky/pre-commit', 'ejs/pre-commit.ejs', hasAnyGitHookTool),
      file('.husky/commit-msg', 'ejs/commit-msg.ejs', hasLintTool('commitlint')),
      file('.czrc.cjs', 'ejs/.czrc.cjs.ejs', hasLintTool('commitlint')),
      // 示例子包：给 monorepo 一个最小可运行的 package，便于用户开箱即用。
      file('packages/core/package.json', 'ejs/packages/core/package.json.ejs'),
      file('packages/core/src/index.ts', 'ejs/packages/core/src/index.ts.ejs', normalized.useTs),
      file('packages/core/src/index.js', 'ejs/packages/core/src/index.js.ejs', !normalized.useTs),
      file('packages/core/vite.config.ts', 'ejs/packages/core/vite.config.ts.ejs', normalized.useTs),
      file('packages/core/vite.config.js', 'ejs/packages/core/vite.config.js.ejs', !normalized.useTs),
      file('packages/core/tsconfig.json', 'ejs/packages/core/tsconfig.json.ejs', normalized.useTs),
      file('.changeset/config.json', 'ejs/.changeset.config.json.ejs'),
      file('.changeset/README.md', 'ejs/.changeset.README.md.ejs'),
    ],
  };
};
