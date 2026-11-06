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
import { choicesFor, presentationRuntimeIds } from './presentation.js';
import { NODE_GIT_HOOK_TOOLS, templateChoicesFor } from './presentation.js';
import { NODE_CHOICES } from './node-choices.js';
import { DART_CHOICES } from './templates/dart-cli/options.js';
import { GO_CHOICES } from './templates/go-service/options.js';
import { PYTHON_CHOICES } from './templates/python-cli/options.js';
import { RUST_CHOICES } from './templates/rust-lib/options.js';
import {
  LINT_TOOL_CHOICES as SPA_REACT_LINT,
  PACKAGE_MANAGER_CHOICES as SPA_REACT_PM,
} from './templates/spa-react/options.js';
import {
  LINT_TOOL_CHOICES as SPA_VUE_LINT,
  PACKAGE_MANAGER_CHOICES as SPA_VUE_PM,
} from './templates/spa-vue/options.js';
import {
  LINT_TOOL_CHOICES as TOOLKIT_LINT,
  PACKAGE_MANAGER_CHOICES as TOOLKIT_PM,
  PACKAGE_MANAGER_DEFAULT as TOOLKIT_PM_DEFAULT,
  UNIT_TEST_TOOL_CHOICES as TOOLKIT_TEST,
} from './templates/toolkit/options.js';
import {
  LINT_TOOL_CHOICES as MONOREPO_LINT,
  PACKAGE_MANAGER_CHOICES as MONOREPO_PM,
  PACKAGE_MANAGER_DEFAULT as MONOREPO_PM_DEFAULT,
  UNIT_TEST_TOOL_CHOICES as MONOREPO_TEST,
} from './templates/toolkit-monorepo/options.js';
import type { ChoiceSpec } from './language-options.js';

/**
 * B14：模板的选项必须**派生**自运行时工具链表（`presentation.ts`），
 * 而不是各自维护一份字面量。这里同时校验两件事：
 * 1. 引用相等（`NODE_CHOICES === choicesFor('node')`）—— 证明是派生而不是复制的副本；
 * 2. 值集合 / 默认值一致 —— 万一有人改回「复制一份字面量」，第 1 条会失败；
 *    万一有人只改了 toolchain 表，模板自动跟随，第 2 条仍然为真。
 */
test('language choice specs derive from the runtime toolchain presentation table', () => {
  const cases: Array<[string, ChoiceSpec]> = [
    ['node', NODE_CHOICES],
    ['go', GO_CHOICES],
    ['python', PYTHON_CHOICES],
    ['rust', RUST_CHOICES],
    ['dart', DART_CHOICES],
  ];

  for (const [runtime, spec] of cases) {
    const presentation = choicesFor(runtime);
    assert.equal(spec, presentation, `${runtime} choices must be the presentation object itself`);

    const values = (options: Array<{ value: string }>) => options.map((option) => option.value);
    assert.deepEqual(values(spec.packageManagers), values(presentation.packageManagers), runtime);
    assert.deepEqual(values(spec.lintTools), values(presentation.lintTools), runtime);
    assert.deepEqual(values(spec.testTools), values(presentation.testTools), runtime);
    assert.deepEqual(spec.defaultLintTools, presentation.defaultLintTools, runtime);
    assert.equal(spec.defaultPackageManager, presentation.defaultPackageManager, runtime);
    assert.equal(spec.defaultTestTool, presentation.defaultTestTool, runtime);

    // 模板提问只认 value；label 是展示用的，缺了会让交互界面变成空白选项。
    for (const option of [...spec.packageManagers, ...spec.lintTools, ...spec.testTools]) {
      assert.ok(option.value.length > 0, `${runtime} option without value`);
      assert.ok(option.label.length > 0, `${runtime} option ${option.value} without label`);
    }
  }
});

test('every template-facing runtime registers a presentation entry', () => {
  // 有模板的运行时必须都能拿到选项；否则 `choicesFor` 会在 create 时报错。
  for (const runtime of ['node', 'go', 'python', 'rust', 'dart']) {
    assert.ok(presentationRuntimeIds().includes(runtime), `missing presentation for ${runtime}`);
  }
  // 未知运行时必须显式失败，而不是静默返回空选项表。
  assert.throws(() => choicesFor('cobol'), /no template presentation registered/);
});

/**
 * 契约快照：模板的 `config.ts` / `dependencies.ts` 里对 lint / test 工具的 value 是**硬编码分支**
 * （例如 go 看到 `gofmt` 才生成格式化配置）。所以「value 集合」是与模板实现的契约，
 * 不能因为改了 presentation 就悄悄变化 —— 那会让用户选了工具却什么配置都不生成。
 *
 * 这条测试与上面的「派生关系」互补：
 * - 派生关系测试（引用相等）挡住「模板又复制一份字面量」；
 * - 契约快照挡住「presentation 被改名，而模板实现没跟上」。
 */
test('presentation value sets stay in sync with what the templates implement', () => {
  const expected: Record<string, { lint: string[]; test: string[]; packageManager: string }> = {
    node: {
      packageManager: 'npm',
      lint: ['eslint', 'prettier', 'oxlint'],
      test: ['vitest', 'skip'],
    },
    go: {
      packageManager: 'go',
      lint: ['golangci-lint', 'gofmt', 'go-vet'],
      test: ['go-test', 'skip'],
    },
    python: {
      packageManager: 'pip',
      lint: ['ruff', 'black', 'mypy', 'flake8'],
      test: ['pytest', 'skip'],
    },
    rust: {
      packageManager: 'cargo',
      lint: ['clippy', 'rustfmt'],
      test: ['builtin', 'skip'],
    },
    dart: {
      packageManager: 'pub',
      lint: ['dart-analyze'],
      test: ['test', 'skip'],
    },
  };

  for (const [runtime, contract] of Object.entries(expected)) {
    const presentation = choicesFor(runtime);
    const values = (options: Array<{ value: string }>) => options.map((option) => option.value);
    assert.equal(presentation.defaultPackageManager, contract.packageManager, runtime);
    assert.deepEqual(values(presentation.lintTools), contract.lint, runtime);
    assert.deepEqual(values(presentation.testTools), contract.test, runtime);
  }
});

/**
 * B14 extras：spa-react / spa-vue / toolkit / toolkit-monorepo 的 lint 工具与包管理器
 * 必须与 `presentation.ts` 的 `NODE_TEMPLATE_CHOICES` **共享同一个数组实例**，
 * 而不是各写一份看起来一样的字面量。这里同时锁住「既有问答顺序」这份行为契约。
 */
test('node template tool lists are derived from the presentation extras', () => {
  const spa = templateChoicesFor('spa');
  const toolkit = templateChoicesFor('toolkit');

  // 严格相等：四个模板共享 presentation 里的同一份 value 数组（复制一份就会失败）。
  assert.equal(SPA_REACT_LINT, spa.lintToolValues);
  assert.equal(SPA_VUE_LINT, spa.lintToolValues);
  assert.equal(SPA_REACT_PM, spa.packageManagerValues);
  assert.equal(SPA_VUE_PM, spa.packageManagerValues);
  assert.equal(TOOLKIT_LINT, toolkit.lintToolValues);
  assert.equal(MONOREPO_LINT, toolkit.lintToolValues);
  assert.equal(TOOLKIT_PM, toolkit.packageManagerValues);
  assert.equal(MONOREPO_PM, toolkit.packageManagerValues);

  // 行为快照：改动前这四个模板的问答顺序/集合（含 stylelint 差异、pnpm 优先）。
  assert.deepEqual(SPA_REACT_LINT, [
    'eslint',
    'oxlint',
    'prettier',
    'oxfmt',
    'stylelint',
    'commitlint',
    'editorconfig',
  ]);
  assert.deepEqual(TOOLKIT_LINT, [
    'eslint',
    'oxlint',
    'prettier',
    'oxfmt',
    'commitlint',
    'editorconfig',
  ]);
  assert.deepEqual(SPA_REACT_PM, ['pnpm', 'npm', 'yarn', 'bun']);
  assert.deepEqual(TOOLKIT_PM, ['pnpm', 'npm', 'yarn', 'bun']);

  // 默认值也来自同一处：toolkit 预勾四项 + 默认 pnpm；spa 不预勾、不写 default。
  assert.deepEqual(toolkit.defaultLintTools, ['eslint', 'prettier', 'commitlint', 'editorconfig']);
  assert.equal(toolkit.defaultPackageManager, 'pnpm');
  assert.deepEqual(spa.defaultLintTools, []);
  assert.equal(spa.defaultPackageManager, null);
  assert.equal(TOOLKIT_PM_DEFAULT, 'pnpm');
  assert.equal(MONOREPO_PM_DEFAULT, 'pnpm');

  // 测试运行器：toolkit 系模板给出的就是 node presentation 的默认测试工具。
  assert.deepEqual(TOOLKIT_TEST, ['vitest']);
  assert.deepEqual(MONOREPO_TEST, ['vitest']);
  assert.equal(TOOLKIT_TEST[0], choicesFor('node').defaultTestTool);

  // label 只定义一次，缺了会让未来 UI 出现空选项。
  for (const option of [
    ...spa.lintTools,
    ...toolkit.lintTools,
    ...spa.packageManagers,
    ...toolkit.packageManagers,
  ]) {
    assert.ok(option.value.length > 0, 'option without value');
    assert.ok(option.label.length > 0, `${option.value} without label`);
  }
});

/**
 * 问答层的防漂移：四个模板真的把这份列表问出来，且默认值/顺序与改动前一致。
 * （`config.ts` 的 git hook 判定名单也来自 `NODE_GIT_HOOK_TOOLS`，这里顺带锁住内容。）
 */
test('node templates ask the same options and defaults as before (B14)', async () => {
  const cases: Array<{
    name: string;
    lint: string[];
    lintDefault: string[];
    pmDefault: string | undefined;
    test: string[];
  }> = [
    {
      name: 'spa-react',
      lint: SPA_REACT_LINT,
      lintDefault: [],
      // spa 现状：包管理器问题不写 default（交互层取第一项 pnpm）。
      pmDefault: undefined,
      test: [],
    },
    {
      name: 'spa-vue',
      lint: SPA_VUE_LINT,
      lintDefault: [],
      pmDefault: undefined,
      test: [],
    },
    {
      name: 'toolkit',
      lint: TOOLKIT_LINT,
      lintDefault: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
      pmDefault: 'pnpm',
      test: ['vitest', 'skip'],
    },
    {
      name: 'toolkit-monorepo',
      lint: MONOREPO_LINT,
      lintDefault: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
      pmDefault: 'pnpm',
      // monorepo 现状：测试运行器只提供 vitest。
      test: ['vitest'],
    },
  ];

  for (const item of cases) {
    const questions = await loadTemplateQuestions(item.name, { cwd: null });
    const lint = questions.find((question) => question.name === 'lintTools');
    assert.ok(lint, `${item.name} missing lintTools question`);
    assert.deepEqual(lint.choices, item.lint, `${item.name} lint choices`);
    assert.deepEqual(lint.default, item.lintDefault, `${item.name} lint default`);

    const packageManager = questions.find((question) => question.name === 'packageManager');
    assert.ok(packageManager, `${item.name} missing packageManager question`);
    assert.deepEqual(packageManager.choices, ['pnpm', 'npm', 'yarn', 'bun']);
    assert.equal(packageManager.default, item.pmDefault, `${item.name} packageManager default`);

    const testTool = questions.find((question) => question.name === 'unitTestTool');
    if (item.test.length === 0) {
      assert.equal(testTool, undefined, `${item.name} must not ask unitTestTool`);
    } else {
      assert.ok(testTool, `${item.name} missing unitTestTool question`);
      assert.deepEqual(testTool.choices, item.test, `${item.name} unitTestTool choices`);
    }
  }

  // git hook 名单：四个 config.ts 共用这一份（顺序无关，只锁集合）。
  assert.deepEqual([...NODE_GIT_HOOK_TOOLS].sort(), [
    'commitlint',
    'eslint',
    'oxfmt',
    'oxlint',
    'prettier',
    'stylelint',
  ]);
});

/**
 * 渲染层的防漂移：`lan.config.ts` 里落下去的 `lintTools` / `packageManager`
 * 必须就是问答里给用户的那份值（含默认勾选）。
 *
 * 注意：`renderRuntimeTemplate` 的 locals 用的是**调用方传入**的 options
 * （交互式流程会把它填成问答答案），所以这里把「问答的默认值」当作答案喂回去，
 * 从而端到端锁住「presentation → 问答默认值 → 生成内容」这条链。
 */
test('node templates render the derived choices into lan.config.ts (B14)', async () => {
  const cases: Array<{ name: string; quotedKeys: boolean; lint: string[] }> = [
    // spa 系：不预勾任何工程化工具（默认 lintTools 为空）。
    { name: 'spa-react', quotedKeys: true, lint: [] },
    { name: 'spa-vue', quotedKeys: true, lint: [] },
    // toolkit 系：默认勾选四项。
    {
      name: 'toolkit',
      quotedKeys: false,
      lint: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
    },
    {
      name: 'toolkit-monorepo',
      quotedKeys: false,
      lint: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
    },
  ];

  for (const item of cases) {
    const questions = await loadTemplateQuestions(item.name, { cwd: null });
    const lintQuestion = questions.find((question) => question.name === 'lintTools');
    const packageManagerQuestion = questions.find((question) => question.name === 'packageManager');
    assert.ok(lintQuestion, `${item.name} missing lintTools question`);
    assert.ok(packageManagerQuestion, `${item.name} missing packageManager question`);

    assert.deepEqual(lintQuestion.default, item.lint, `${item.name} default lint tools`);
    // 没有显式 default 时（spa），交互层会选中第一项；这里照此模拟。
    const packageManagerAnswer =
      (packageManagerQuestion.default as string | undefined) ?? packageManagerQuestion.choices?.[0];
    assert.equal(packageManagerAnswer, 'pnpm', `${item.name} default package manager`);

    const rendered = await renderRuntimeTemplate(item.name, {
      cwd: null,
      context: { projectName: 'demo' },
      options: { lintTools: lintQuestion.default, packageManager: packageManagerAnswer },
    });
    const lanConfig = rendered.files.find((file) => file.path === 'lan.config.ts');
    assert.ok(lanConfig, `${item.name} missing lan.config.ts`);

    const lintSnippet = item.quotedKeys
      ? `"lintTools": ${JSON.stringify(item.lint)}`
      : `lintTools: ${JSON.stringify(item.lint)}`;
    assert.ok(lanConfig.content.includes(lintSnippet), `${item.name} missing ${lintSnippet}`);
    assert.ok(
      lanConfig.content.includes('"packageManager": "pnpm"') ||
        lanConfig.content.includes('packageManager: "pnpm"'),
      `${item.name} missing packageManager in lan.config.ts`,
    );
  }

  // 显式选择的工具必须原样落地，未选的不能因为旧分支而混进来。
  const explicit = await renderRuntimeTemplate('spa-react', {
    cwd: null,
    context: { projectName: 'demo' },
    options: { packageManager: 'pnpm', lintTools: ['eslint', 'stylelint'] },
  });
  const explicitLanConfig = explicit.files.find((file) => file.path === 'lan.config.ts');
  assert.ok(explicitLanConfig);
  assert.ok(
    explicitLanConfig.content.includes('"lintTools": ["eslint","stylelint"]'),
    explicitLanConfig.content,
  );
});

test('template list exposes schema metadata', async () => {
  const result = await loadListResult({});

  assert.deepEqual(result.templates, [
    'dart-cli',
    'express',
    'go-service',
    'nestjs',
    'python-cli',
    'rust-lib',
    'spa-react',
    'spa-vue',
    'toolkit',
    'toolkit-monorepo',
  ]);
  assert.ok(result.metadata.every((template) => typeof template.schemaVersion === 'number'));
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
      (file) => file.path === 'src/index.ts' && file.content.includes('getToolkitInfo'),
    ),
  );
  assert.ok(
    result.files.some(
      (file) => file.path === 'package.json' && file.content.includes('"name": "demo-kit"'),
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
        !file.content.includes(
          '**/*.{js,jsx,ts,tsx,mjs,cjs,json,jsonc,json5,md,mdx,yml,yaml,toml,html,css,scss,less,vue,gql,graphql}',
        ),
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
        file.content.includes(
          '"lint": "oxlint . && eslint . && oxfmt --check . && prettier --check ."',
        ) &&
        file.content.includes(
          '"lint:fix": "oxlint --fix . && eslint . --fix && oxfmt . && prettier --write ."',
        ),
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

  assert.deepEqual(result.dependencies, ['react', 'react-dom']);
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

  assert.deepEqual(result.dependencies, ['vue']);
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
  assert.ok(result.files.some((file) => file.path === 'lan.config.ts'));
  assert.ok(result.files.some((file) => file.path === '.husky/commit-msg'));
  assert.ok(result.files.some((file) => file.path === '.editorconfig'));
  assert.ok(!result.files.some((file) => file.path === 'src/index.js'));
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'package.json' &&
        file.content.includes('"name": "demo-toolkit"') &&
        file.content.includes(
          '"release": "lan release run --profile package --apply --yes --publish"',
        ),
    ),
  );
  assert.ok(
    result.files.some(
      (file) =>
        file.path === 'lan.config.ts' &&
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
        file.path === 'lan.config.ts' &&
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

test('add template runtime renders vue 3 component with style language', async () => {
  const result = await renderAddRuntimeTemplate('v3', {
    context: {
      projectName: 'demo-kit',
      language: 'ts',
      cssProcessor: 'sass',
    },
  });

  assert.equal(result.template, 'v3');
  assert.equal(result.filename, null);
  assert.equal(result.extname, 'vue');
  assert.match(result.content, /<script setup lang="ts">/);
  assert.match(result.content, /<style scoped lang="scss">/);
});

test('template runtime scans project template directories and normalizes modules', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-template-runtime-'));
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

test('go-service template renders go manifest and defaults skipInstall', async () => {
  const names = (await listRuntimeTemplates({})).map((template) => template.manifest.name);
  assert.ok(names.includes('go-service'));

  const questions = await loadTemplateQuestions('go-service', { cwd: null });
  const skipInstall = questions.find((question) => question.name === 'skipInstall');
  assert.equal(skipInstall?.type, 'confirm');
  assert.equal(skipInstall?.default, true);

  const rendered = await renderRuntimeTemplate('go-service', {
    cwd: null,
    context: { projectName: 'demo', modulePath: 'example.com/demo' },
  });
  const paths = rendered.files.map((file) => file.path);
  assert.ok(paths.includes('go.mod'));
  assert.ok(paths.includes('main.go'));
  assert.ok(
    rendered.files.some(
      (file) => file.path === 'go.mod' && file.content.includes('module example.com/demo'),
    ),
  );
});

test('language create templates render their manifests', async () => {
  const cases: Array<[string, string]> = [
    ['rust-lib', 'Cargo.toml'],
    ['python-cli', 'pyproject.toml'],
    ['dart-cli', 'pubspec.yaml'],
  ];
  for (const [name, manifest] of cases) {
    const rendered = await renderRuntimeTemplate(name, {
      cwd: null,
      context: { projectName: 'demo' },
    });
    const paths = rendered.files.map((file) => file.path);
    assert.ok(paths.includes(manifest), `${name} missing ${manifest}`);
    const questions = await loadTemplateQuestions(name, { cwd: null });
    assert.ok(
      questions.some((question) => question.name === 'skipInstall' && question.default === true),
    );
  }
});

test('language templates expose package manager lint and test choices', async () => {
  const cases: Array<{
    name: string;
    lint: string;
    test: string;
    extraFiles: string[];
    absentFiles: string[];
  }> = [
    {
      name: 'python-cli',
      lint: 'ruff',
      test: 'pytest',
      extraFiles: ['ruff.toml', 'tests/test_main.py'],
      absentFiles: ['.flake8'],
    },
    {
      name: 'go-service',
      lint: 'golangci-lint',
      test: 'go-test',
      extraFiles: ['.golangci.yml', 'main_test.go'],
      absentFiles: [],
    },
    {
      name: 'rust-lib',
      lint: 'clippy',
      test: 'builtin',
      extraFiles: ['clippy.toml', 'tests/smoke.rs'],
      absentFiles: [],
    },
    {
      name: 'dart-cli',
      lint: 'dart-analyze',
      test: 'test',
      extraFiles: ['analysis_options.yaml', 'test/main_test.dart'],
      absentFiles: [],
    },
  ];

  for (const item of cases) {
    const questions = await loadTemplateQuestions(item.name, { cwd: null });
    assert.ok(
      questions.some((question) => question.name === 'lintTools'),
      item.name,
    );
    assert.ok(
      questions.some((question) => question.name === 'testTool'),
      item.name,
    );

    const rendered = await renderRuntimeTemplate(item.name, {
      cwd: null,
      context: { projectName: 'demo', packageName: 'demo', modulePath: 'example.com/demo' },
      options: {
        packageManager: item.name === 'python-cli' ? 'pip' : undefined,
        lintTools: [item.lint],
        testTool: item.test,
      },
    });
    const paths = rendered.files.map((file) => file.path);
    for (const expected of item.extraFiles) {
      assert.ok(paths.includes(expected), `${item.name} missing ${expected}`);
    }
    for (const absent of item.absentFiles) {
      assert.ok(!paths.includes(absent), `${item.name} should not include ${absent}`);
    }
    const lanConfig = rendered.files.find((file) => file.path === 'lan.config.ts');
    assert.ok(lanConfig?.content.includes('lintTools'), `${item.name} missing lintTools`);
    assert.ok(lanConfig?.content.includes(item.lint), `${item.name} missing ${item.lint}`);
  }
});

test('python poetry template renders poetry metadata', async () => {
  const rendered = await renderRuntimeTemplate('python-cli', {
    cwd: null,
    context: { projectName: 'demo', packageName: 'demo' },
    options: { packageManager: 'poetry', lintTools: ['ruff'], testTool: 'pytest' },
  });
  const paths = rendered.files.map((file) => file.path);
  assert.ok(!paths.includes('requirements-dev.txt'));
  assert.ok(
    rendered.files.some(
      (file) => file.path === 'pyproject.toml' && file.content.includes('[tool.poetry]'),
    ),
  );
});

test('node framework templates render package manifests and tool configs', async () => {
  const cases: Array<[string, string]> = [
    ['nestjs', '@nestjs/core'],
    ['express', 'express'],
  ];
  for (const [name, dependency] of cases) {
    const rendered = await renderRuntimeTemplate(name, {
      cwd: null,
      context: { projectName: 'demo', packageName: 'demo' },
      options: {
        packageManager: 'pnpm',
        lintTools: ['eslint', 'prettier'],
        testTool: 'vitest',
      },
    });
    const paths = rendered.files.map((file) => file.path);
    assert.ok(paths.includes('package.json'), name);
    assert.ok(paths.includes('eslint.config.js'), name);
    assert.ok(paths.includes('prettier.config.cjs'), name);
    assert.ok(paths.includes('test/app.test.ts'), name);
    const packageJson = rendered.files.find((file) => file.path === 'package.json');
    assert.ok(packageJson?.content.includes(`"${dependency}"`), name);
    const lanConfig = rendered.files.find((file) => file.path === 'lan.config.ts');
    assert.ok(lanConfig?.content.includes('buildTool'), `${name} missing buildTool`);
    assert.ok(lanConfig?.content.includes('lintTools'), `${name} missing lintTools`);
  }
});

test('language add templates render the requested component name', async () => {
  const cases: Array<[string, string]> = [
    ['go-handler', 'go'],
    ['python-module', 'py'],
    ['rust-module', 'rs'],
    ['dart-class', 'dart'],
  ];
  for (const [name, ext] of cases) {
    const rendered = await renderAddRuntimeTemplate(name, {
      context: { componentName: 'Widget', language: 'ts' },
    });
    assert.equal(rendered.extname, ext);
    assert.ok(rendered.content.includes('Widget'), `${name} missing component name`);
  }
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
