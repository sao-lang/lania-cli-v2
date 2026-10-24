import { normalizeGoServiceOptions } from './options.js';

interface FileRule {
  outputPath: string;
  templatePath: string;
}

function file(outputPath: string, templatePath: string, enabled = true): FileRule | null {
  return enabled ? { outputPath, templatePath } : null;
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeGoServiceOptions(options);
  const hasLint = (tool: string) => normalized.lintTools.includes(tool);
  const files = [
    file('go.mod', 'ejs/go.mod.ejs'),
    file('main.go', 'ejs/main.go.ejs'),
    file('.golangci.yml', 'ejs/.golangci.yml.ejs', hasLint('golangci-lint')),
    file('main_test.go', 'ejs/main_test.go.ejs', normalized.testTool === 'go-test'),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('README.md', 'ejs/README.md.ejs'),
  ].filter((item): item is FileRule => item !== null);

  return {
    outputTasks: ['write-files'],
    files,
  };
};
