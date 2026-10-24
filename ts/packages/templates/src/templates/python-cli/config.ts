import { normalizePythonCliOptions } from './options.js';

interface FileRule {
  outputPath: string;
  templatePath: string;
}

function file(outputPath: string, templatePath: string, enabled = true): FileRule | null {
  return enabled ? { outputPath, templatePath } : null;
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizePythonCliOptions(options);
  const hasLint = (tool: string) => normalized.lintTools.includes(tool);
  const files = [
    file('pyproject.toml', 'ejs/pyproject.toml.ejs'),
    file('src/main.py', 'ejs/src/main.py.ejs'),
    file('requirements.txt', 'ejs/requirements.txt.ejs'),
    file(
      'requirements-dev.txt',
      'ejs/requirements-dev.txt.ejs',
      normalized.packageManager !== 'poetry',
    ),
    file('ruff.toml', 'ejs/ruff.toml.ejs', hasLint('ruff')),
    file('.flake8', 'ejs/.flake8.ejs', hasLint('flake8')),
    file('mypy.ini', 'ejs/mypy.ini.ejs', hasLint('mypy')),
    file('tests/test_main.py', 'ejs/tests/test_main.py.ejs', normalized.testTool === 'pytest'),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('README.md', 'ejs/README.md.ejs'),
  ].filter((item): item is FileRule => item !== null);

  return {
    outputTasks: ['write-files'],
    files,
  };
};
