/**
 * toolkit 模板的输出文件规则与渲染配置。
 *
 * 输出策略：
 * - 保持单包工具库结构尽量精简，只生成一个 `src/index` 入口与基础 vite 配置
 * - TypeScript / JavaScript、lint 工具、git hooks 相关文件都按 normalized options 条件开启
 */
import { normalizeToolkitOptions } from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitOptions(options);
  const hasLintTool = (tool: string) => normalized.lintTools.includes(tool);
  // 这些工具通常意味着会落地 pre-commit / commit-msg hook。
  const hasAnyGitHookTool = ['eslint', 'oxlint', 'prettier', 'oxfmt', 'stylelint', 'commitlint'].some((tool) =>
    normalized.lintTools.includes(tool),
  );
  const files = [
    file('README.md', 'files/README.md.ejs'),
    file('vite.config.ts', 'ejs/vite.config.ts.ejs', normalized.useTs),
    file('vite.config.js', 'ejs/vite.config.js.ejs', !normalized.useTs),
    file('tsconfig.json', 'ejs/tsconfig.json.ejs', normalized.useTs),
    file('package.json', 'ejs/package.json.ejs'),
    file('src/index.ts', 'ejs/index.ts.ejs', normalized.useTs),
    file('src/index.js', 'ejs/index.js.ejs', !normalized.useTs),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('eslint.config.js', 'ejs/eslint.config.js.ejs', hasLintTool('eslint')),
    file('.oxlintrc.json', 'ejs/.oxlintrc.json.ejs', hasLintTool('oxlint')),
    file('prettier.config.cjs', 'ejs/prettier.config.cjs.ejs', hasLintTool('prettier')),
    file('.oxfmtrc.json', 'ejs/.oxfmtrc.json.ejs', hasLintTool('oxfmt')),
    file('commitlint.config.cjs', 'ejs/commitlint.config.cjs.ejs', hasLintTool('commitlint')),
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
