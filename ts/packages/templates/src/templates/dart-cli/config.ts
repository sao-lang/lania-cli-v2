import { normalizeDartCliOptions } from './options.js';

interface FileRule {
  outputPath: string;
  templatePath: string;
}

function file(outputPath: string, templatePath: string, enabled = true): FileRule | null {
  return enabled ? { outputPath, templatePath } : null;
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeDartCliOptions(options);
  const files = [
    file('pubspec.yaml', 'ejs/pubspec.yaml.ejs'),
    file('bin/main.dart', 'ejs/bin/main.dart.ejs'),
    file(
      'analysis_options.yaml',
      'ejs/analysis_options.yaml.ejs',
      normalized.lintTools.includes('dart-analyze'),
    ),
    file('test/main_test.dart', 'ejs/test/main_test.dart.ejs', normalized.testTool === 'test'),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('README.md', 'ejs/README.md.ejs'),
  ].filter((item): item is FileRule => item !== null);

  return {
    outputTasks: ['write-files'],
    files,
  };
};
