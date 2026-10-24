import { normalizeNestjsOptions } from './options.js';

interface FileRule {
  outputPath: string;
  templatePath: string;
}

function file(outputPath: string, templatePath: string, enabled = true): FileRule | null {
  return enabled ? { outputPath, templatePath } : null;
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeNestjsOptions(options);
  const hasLint = (tool: string) => normalized.lintTools.includes(tool);
  const files = [
    file('package.json', 'ejs/package.json.ejs'),
    file('tsconfig.json', 'ejs/tsconfig.json.ejs'),
    file('nest-cli.json', 'ejs/nest-cli.json.ejs'),
    file('src/main.ts', 'ejs/src/main.ts.ejs'),
    file('src/app.module.ts', 'ejs/src/app.module.ts.ejs'),
    file('src/app.controller.ts', 'ejs/src/app.controller.ts.ejs'),
    file('src/app.service.ts', 'ejs/src/app.service.ts.ejs'),
    file('test/app.test.ts', 'ejs/test/app.test.ts.ejs', normalized.testTool === 'vitest'),
    file('eslint.config.js', 'ejs/eslint.config.js.ejs', hasLint('eslint')),
    file('prettier.config.cjs', 'ejs/prettier.config.cjs.ejs', hasLint('prettier')),
    file('.oxlintrc.json', 'ejs/.oxlintrc.json.ejs', hasLint('oxlint')),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('README.md', 'ejs/README.md.ejs'),
  ].filter((item): item is FileRule => item !== null);

  return {
    outputTasks: ['write-files', 'install-deps'],
    files,
  };
};
