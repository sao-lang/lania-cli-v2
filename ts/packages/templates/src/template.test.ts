/**
 * 模板运行时的回归测试。
 *
 * 主要导出：widgetName。
 * 关键点：
 * - 包含文件系统读写/路径解析
 * - 包含 JSON 协议/序列化
 */
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  renderAddRuntimeTemplate,
  listRuntimeTemplates,
  loadTemplateDependencies,
  loadTemplateOutputTasks,
  loadTemplateQuestions,
  renderRuntimeTemplate,
  type TemplateManifest,
} from './index.js';

test('template list exposes schema metadata', async () => {
  const result = await loadListResult({});

  assert.deepEqual(result.templates, ['spa-react', 'spa-vue', 'toolkit', 'toolkit-monorepo']);
  assert.ok(
    result.metadata.every((template) => typeof template.schemaVersion === 'number'),
  );
  assert.equal(
    result.metadata.find((template) => template.name === 'spa-react')?.renderEngine,
    'rust_declarative',
  );
  assert.equal(
    result.metadata.find((template) => template.name === 'toolkit')?.renderEngine,
    'node_bridge',
  );
  assert.equal(
    result.metadata.find((template) => template.name === 'toolkit-monorepo')?.renderEngine,
    'node_bridge',
  );
});

test('template render returns schema version and engine', async () => {
  const result = await renderRuntimeTemplate('toolkit', {
    context: {
      projectName: 'demo-kit',
    },
  });

  assert.equal(result.template, 'toolkit');
  assert.equal(result.schemaVersion, 1);
  assert.equal(result.renderEngine, 'node_bridge');
  assert.ok(result.files.some((file) => file.path === 'src/index.ts'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'src/index.ts' &&
        file.content.includes('getToolkitInfo'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"name": "demo-kit"'),
    ),
  );
});

test('spa-react template renders the migrated legacy ejs asset set', async () => {
  const result = await renderRuntimeTemplate('spa-react', {
    context: {
      projectName: 'demo-react',
    },
    options: {
      buildTool: 'webpack',
      language: 'javascript',
      useTs: false,
      cssProcessor: 'less',
      cssTools: ['tailwindcss'],
      lintTools: ['eslint', 'prettier', 'stylelint', 'commitlint', 'editorconfig'],
      packageManager: 'pnpm',
      repository: 'git@example.com/demo-react.git',
      port: 4100,
    },
  });

  assert.equal(result.template, 'spa-react');
  assert.ok(result.files.length >= 20);
  assert.ok(result.files.some((file) => file.path === 'webpack.config.cjs'));
  assert.ok(result.files.some((file) => file.path === 'tailwind.config.cjs'));
  assert.ok(result.files.some((file) => file.path === 'postcss.config.cjs'));
  assert.ok(result.files.some((file) => file.path === '.husky/commit-msg'));
  assert.ok(result.files.some((file) => file.path === '.husky/pre-commit'));
  assert.ok(result.files.some((file) => file.path === '.editorconfig'));
  assert.ok(result.files.some((file) => file.path === 'src/App.jsx'));
  assert.ok(result.files.some((file) => file.path === 'src/main.jsx'));
  assert.ok(result.files.some((file) => file.path === 'src/App.less'));
  assert.ok(result.files.some((file) => file.path === 'src/index.less'));
  assert.ok(!result.files.some((file) => file.path === 'vite.config.ts'));
  assert.ok(!result.files.some((file) => file.path === 'src/main.tsx'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"name": "demo-react"') &&
        file.content.includes('"dev": "lan dev"'),
    ),
  );
});

test('spa-react oxlint+eslint+oxfmt+prettier produces ox configs and avoids prettier-in-eslint', async () => {
  const result = await renderRuntimeTemplate('spa-react', {
    context: { projectName: 'demo-react-ox' },
    options: {
      buildTool: 'vite',
      useTs: true,
      cssProcessor: 'css',
      cssTools: [],
      lintTools: ['oxlint', 'eslint', 'oxfmt', 'prettier'],
      packageManager: 'pnpm',
      repository: '',
      port: 3000,
    },
  });

  assert.ok(result.files.some((file) => file.path === '.oxlintrc.json'));
  assert.ok(result.files.some((file) => file.path === '.oxfmtrc.json'));
  assert.ok(result.files.some((file) => file.path === 'eslint.config.js'));
  assert.ok(result.files.some((file) => file.path === '.prettierignore'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === '.prettierignore' &&
        !file.content.includes('**/*.{js,jsx,ts,tsx,mjs,cjs,json,jsonc,json5,md,mdx,yml,yaml,toml,html,css,scss,less,vue,gql,graphql}'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'eslint.config.js' &&
        !file.content.includes('eslint-plugin-prettier') &&
        file.content.includes('eslint-plugin-oxlint'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"lint": "oxlint . && eslint . && oxfmt --check . && prettier --check ."') &&
        file.content.includes('"lint:fix": "oxlint --fix . && eslint . --fix && oxfmt . && prettier --write ."'),
    ),
  );
});

test('spa-vue template renders the migrated legacy ejs asset set', async () => {
  const result = await renderRuntimeTemplate('spa-vue', {
    context: {
      projectName: 'demo-vue',
    },
    options: {
      buildTool: 'webpack',
      language: 'javascript',
      useTs: false,
      cssProcessor: 'less',
      cssTools: ['tailwindcss'],
      lintTools: ['eslint', 'prettier', 'stylelint', 'commitlint', 'editorconfig'],
      packageManager: 'pnpm',
      repository: 'git@example.com/demo-vue.git',
      port: 4200,
    },
  });

  assert.equal(result.template, 'spa-vue');
  assert.ok(result.files.length >= 20);
  assert.ok(result.files.some((file) => file.path === 'webpack.config.cjs'));
  assert.ok(result.files.some((file) => file.path === 'tailwind.config.cjs'));
  assert.ok(result.files.some((file) => file.path === 'src/App.vue'));
  assert.ok(result.files.some((file) => file.path === 'src/main.js'));
  assert.ok(result.files.some((file) => file.path === 'src/App.less'));
  assert.ok(result.files.some((file) => file.path === '.husky/pre-commit'));
  assert.ok(!result.files.some((file) => file.path === 'vite.config.ts'));
  assert.ok(!result.files.some((file) => file.path === 'src/main.ts'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"name": "demo-vue"') &&
        file.content.includes('"dev": "lan dev"'),
    ),
  );
});

test('spa-react webpack dependencies include webpack-dev-server', async () => {
  const result = await loadTemplateDependencies('spa-react', {
    context: {
      projectName: 'demo-react',
    },
    options: {
      buildTool: 'webpack',
      useTs: true,
      lintTools: ['eslint'],
    },
  });

  assert.ok(result.devDependencies.includes('webpack'));
  assert.ok(result.devDependencies.includes('webpack-dev-server'));
});

test('spa-vue webpack dependencies include webpack-dev-server', async () => {
  const result = await loadTemplateDependencies('spa-vue', {
    context: {
      projectName: 'demo-vue',
    },
    options: {
      buildTool: 'webpack',
      useTs: true,
      lintTools: ['eslint'],
    },
  });

  assert.ok(result.devDependencies.includes('webpack'));
  assert.ok(result.devDependencies.includes('webpack-dev-server'));
});

test('toolkit template renders the migrated legacy ejs asset set', async () => {
  const result = await renderRuntimeTemplate('toolkit', {
    context: {
      projectName: 'demo-toolkit',
      packageManager: 'pnpm',
    },
    options: {
      useTs: true,
      lintTools: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
      repository: 'git@example.com/demo-toolkit.git',
      unitTestTool: 'vitest',
    },
  });

  assert.equal(result.template, 'toolkit');
  assert.ok(result.files.length >= 12);
  assert.ok(result.files.some((file) => file.path === 'vite.config.ts'));
  assert.ok(result.files.some((file) => file.path === 'tsconfig.json'));
  assert.ok(result.files.some((file) => file.path === 'src/index.ts'));
  assert.ok(result.files.some((file) => file.path === 'lan.config.js'));
  assert.ok(result.files.some((file) => file.path === '.husky/commit-msg'));
  assert.ok(result.files.some((file) => file.path === '.editorconfig'));
  assert.ok(!result.files.some((file) => file.path === 'src/index.js'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"name": "demo-toolkit"') &&
        file.content.includes('"release": "lan release run --profile package --apply --yes --publish"'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'lan.config.js' &&
        file.content.includes('command: "pnpm version"') &&
        file.content.includes('command: "pnpm run changelog"'),
    ),
  );
});

test('toolkit oxlint+eslint uses eslint-plugin-oxlint and does not rely on lan lint', async () => {
  const result = await renderRuntimeTemplate('toolkit', {
    context: {
      projectName: 'demo-toolkit-ox',
      packageManager: 'pnpm',
    },
    options: {
      useTs: true,
      lintTools: ['oxlint', 'eslint'],
      repository: '',
      unitTestTool: 'vitest',
    },
  });

  assert.ok(result.files.some((file) => file.path === '.oxlintrc.json'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'eslint.config.js' &&
        file.content.includes('eslint-plugin-oxlint') &&
        file.content.includes("buildFromOxlintConfigFile('./.oxlintrc.json')"),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"lint": "oxlint . && eslint ."') &&
        !file.content.includes('lan lint'),
    ),
  );
});

test('toolkit-monorepo template renders workspace and changesets defaults', async () => {
  const result = await renderRuntimeTemplate('toolkit-monorepo', {
    context: {
      projectName: 'demo-kit-workspace',
      packageManager: 'pnpm',
    },
    options: {
      useTs: true,
      lintTools: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
      repository: 'git@example.com/demo-kit-workspace.git',
      unitTestTool: 'vitest',
    },
  });

  assert.equal(result.template, 'toolkit-monorepo');
  assert.ok(result.files.some((file) => file.path === 'package.json'));
  assert.ok(result.files.some((file) => file.path === 'pnpm-workspace.yaml'));
  assert.ok(result.files.some((file) => file.path === '.changeset/config.json'));
  assert.ok(result.files.some((file) => file.path === 'packages/core/package.json'));
  assert.ok(result.files.some((file) => file.path === 'packages/core/src/index.ts'));
  assert.ok(result.files.some((file) => file.path === 'scripts/run-package.mjs'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"version-packages": "changeset version"') &&
        file.content.includes('"release": "lan release run --profile custom --apply --yes"'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'lan.config.js' &&
        file.content.includes('provider: "changesets"') &&
        file.content.includes('command: "pnpm run release:publish"'),
    ),
  );
});

test('add template runtime restores legacy add template assets', async () => {
  const result = await renderAddRuntimeTemplate('rfc', {
    context: {
      projectName: 'demo-kit',
      language: 'ts',
      cssProcessor: 'scss',
    },
  });

  assert.equal(result.template, 'rfc');
  assert.equal(result.filename, null);
  assert.equal(result.extname, 'tsx');
  assert.match(result.content, /const MyComponent/);
});

test('template runtime scans project template directories and normalizes modules', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-template-runtime-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const runtimeDir = path.join(cwd, 'template-runtime');
  const templateDir = path.join(runtimeDir, 'custom-widget');
  await mkdir(path.join(templateDir, 'files'), { recursive: true });

  await writeFile(
    path.join(cwd, 'lan.config.js'),
    [
      'export default {',
      "  templateRuntimeDirs: ['./template-runtime'],",
      '};',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'template.json'),
    JSON.stringify(
      {
        name: 'custom-widget',
        schemaVersion: 1,
        renderEngine: 'node_bridge',
        legacyTemplateDir: 'third-party/custom-widget',
        ownership: 'third_party_extension',
        useCases: ['create', 'add'],
        migrationLayer: 'third_party_template_extension',
      },
      null,
      2,
    ),
  );
  await writeFile(
    path.join(templateDir, 'questions.ts'),
    [
      'export default ({ options = {} }) => [',
      "  { name: 'projectName', type: 'input', message: 'Project name' },",
      '  ...(options.includeFlavor ? [{ name: \"flavor\", type: \"select\", choices: [\"basic\", \"pro\"] }] : []),',
      '];',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'dependencies.ts'),
    [
      'export default ({ options = {} }) => ({',
      "  dependencies: ['lit'],",
      "  devDependencies: ['typescript', ...(options.includeTests ? ['vitest'] : [])],",
      '});',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'config.ts'),
    [
      'export default ({ context = {}, options = {} }) => ({',
      "  outputTasks: ['write-files', ...(options.install === false ? [] : ['install-deps'])],",
      '  files: [',
      "    { outputPath: 'package.json', templatePath: 'files/package.json.ejs' },",
      "    ...(context.projectName ? [{ outputPath: 'src/widget.ts', templatePath: 'files/widget.ts.ejs' }] : []),",
      '  ],',
      '});',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'files', 'package.json.ejs'),
    '{\n  "name": "<%= projectName %>"\n}\n',
  );
  await writeFile(
    path.join(templateDir, 'files', 'widget.ts.ejs'),
    "export const widgetName = '<%= projectName %>';\n",
  );

  const listResult = await loadListResult({
    cwd,
  });
  assert.ok(listResult.templates.includes('custom-widget'));
  assert.ok(
    listResult.metadata.some(
      (template) =>
        template.name === 'custom-widget' &&
        template.renderEngine === 'node_bridge' &&
        template.migrationLayer === 'third_party_template_extension',
    ),
  );

  const questions = await loadTemplateQuestions('custom-widget', {
    cwd,
    options: {
      includeFlavor: true,
    },
  });
  assert.equal(questions[1]?.name, 'flavor');

  const depsResult = await loadTemplateDependencies('custom-widget', {
    cwd,
    options: {
      includeTests: true,
    },
  });
  assert.deepEqual(depsResult.dependencies, ['lit']);
  assert.deepEqual(depsResult.devDependencies, ['typescript', 'vitest']);

  const tasksResult = await loadTemplateOutputTasks('custom-widget', {
    cwd,
    options: {
      install: false,
    },
  });
  assert.deepEqual(tasksResult, ['write-files']);

  const renderResult = await renderRuntimeTemplate('custom-widget', {
    cwd,
    context: {
      projectName: 'widget-demo',
    },
  });
  assert.equal(renderResult.renderEngine, 'node_bridge');
  assert.deepEqual(
    renderResult.files.map((file) => file.path),
    ['package.json', 'src/widget.ts'],
  );
  assert.ok(
    renderResult.files.some(
      (file) =>
        file.path === 'src/widget.ts' &&
        file.content.includes("export const widgetName = 'widget-demo';"),
    ),
  );
});

test('template runtime prefers executable js modules when ts variants also exist', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-template-runtime-js-prefer-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const runtimeDir = path.join(cwd, 'template-runtime');
  const templateDir = path.join(runtimeDir, 'custom-widget');
  await mkdir(path.join(templateDir, 'files'), { recursive: true });

  await writeFile(
    path.join(cwd, 'lan.config.js'),
    ['export default {', "  templateRuntimeDirs: ['./template-runtime'],", '};', ''].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'template.json'),
    JSON.stringify(
      {
        name: 'custom-widget',
        schemaVersion: 1,
        renderEngine: 'node_bridge',
        legacyTemplateDir: 'third-party/custom-widget',
        ownership: 'third_party_extension',
        useCases: ['create'],
      },
      null,
      2,
    ),
  );
  await writeFile(
    path.join(templateDir, 'questions.cjs'),
    'module.exports = () => [{ name: "entry", type: "input", message: "Entry" }];\n',
  );
  await writeFile(
    path.join(templateDir, 'questions.ts'),
    'export default () => [{ name: "wrong", type: "input", message: "Should not load ts" }];\n',
  );
  await writeFile(
    path.join(templateDir, 'dependencies.cjs'),
    'module.exports = () => ({ dependencies: ["lit"], devDependencies: ["typescript"] });\n',
  );
  await writeFile(
    path.join(templateDir, 'dependencies.ts'),
    'export default () => ({ dependencies: ["broken"], devDependencies: [] });\n',
  );
  await writeFile(
    path.join(templateDir, 'config.cjs'),
    [
      'module.exports = () => ({',
      '  outputTasks: ["write-files"],',
      '  files: [{ outputPath: "package.json", templatePath: "files/package.json.ejs" }],',
      '});',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'config.ts'),
    [
      'export default () => ({',
      '  outputTasks: ["wrong-task"],',
      '  files: [{ outputPath: "wrong.txt", templatePath: "files/package.json.ejs" }],',
      '});',
      '',
    ].join('\n'),
  );
  await writeFile(
    path.join(templateDir, 'files', 'package.json.ejs'),
    '{\n  "name": "<%= projectName %>"\n}\n',
  );

  const questions = await loadTemplateQuestions('custom-widget', { cwd });
  assert.equal(questions[0]?.name, 'entry');

  const dependencies = await loadTemplateDependencies('custom-widget', { cwd });
  assert.deepEqual(dependencies.dependencies, ['lit']);

  const outputTasks = await loadTemplateOutputTasks('custom-widget', { cwd });
  assert.deepEqual(outputTasks, ['write-files']);

  const rendered = await renderRuntimeTemplate('custom-widget', {
    cwd,
    context: { projectName: 'demo-js-preferred' },
  });
  assert.equal(rendered.files.length, 1);
  assert.equal(rendered.files[0]?.path, 'package.json');
});

async function loadListResult(input: {
  cwd?: string | null;
  context?: Record<string, unknown>;
  options?: Record<string, unknown>;
}) {
  const templates = await listRuntimeTemplates(input);
  return {
    templates: templates.map((template) => template.manifest.name),
    metadata: templates.map((template) => toMetadata(template.manifest)),
  };
}

function toMetadata(template: TemplateManifest) {
  return {
    name: template.name,
    schemaVersion: template.schemaVersion,
    renderEngine: template.renderEngine,
    legacyTemplateDir: template.legacyTemplateDir,
    ownership: template.ownership,
    useCases: template.useCases,
    migrationLayer: template.migrationLayer ?? 'unknown',
  };
}
