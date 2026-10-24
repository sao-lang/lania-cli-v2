import { normalizeRustLibOptions } from './options.js';

interface FileRule {
  outputPath: string;
  templatePath: string;
}

function file(outputPath: string, templatePath: string, enabled = true): FileRule | null {
  return enabled ? { outputPath, templatePath } : null;
}

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeRustLibOptions(options);
  const hasLint = (tool: string) => normalized.lintTools.includes(tool);
  const files = [
    file('Cargo.toml', 'ejs/Cargo.toml.ejs'),
    file('src/lib.rs', 'ejs/src/lib.rs.ejs'),
    file('rustfmt.toml', 'ejs/rustfmt.toml.ejs', hasLint('rustfmt')),
    file('clippy.toml', 'ejs/clippy.toml.ejs', hasLint('clippy')),
    file('tests/smoke.rs', 'ejs/tests/smoke.rs.ejs', normalized.testTool === 'builtin'),
    file('lan.config.js', 'ejs/lan.config.js.ejs'),
    file('.gitignore', 'ejs/.gitignore.ejs'),
    file('README.md', 'ejs/README.md.ejs'),
  ].filter((item): item is FileRule => item !== null);

  return {
    outputTasks: ['write-files'],
    files,
  };
};
