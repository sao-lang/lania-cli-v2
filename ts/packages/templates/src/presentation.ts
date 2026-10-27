/**
 * 运行时 → 模板提问所需知识的唯一来源（设计文档 B14 `RuntimeToolchain.presentation`）。
 *
 * 为什么这个文件在 `templates` 而不是 `node-bridge`：
 * 模板的 `options.ts` / `questions.ts` 在 **模块加载期** 就要读到这些选项，因此必须是
 * 静态 import；而 `templates` 是 `node-bridge` 的依赖（反向 import 会形成 workspace
 * 包环，`pnpm -r build` 的顺序也会失去意义）。所以「模板侧知识」放在 templates，
 * 由 `node-bridge/src/plugins/toolchain.ts` 组合进 `RuntimeToolchain.presentation`，
 * 对 bridge / Rust / `toolchain.describe` 来说仍然是同一份数据、同一个字段。
 *
 * 与 `RUNTIME_TOOLCHAINS[].lintTools` / `.packageManagers` 的关系：
 * - `toolchain.ts` 里的 `lintTools` / `packageManagers` 服务于 **lint / build 分派**
 *   （id、binary、args、inProcess、runner），语义不能改；
 * - 这里的 presentation 服务于 **模板提问与归一化**（value + label + 默认值）。
 * 两者天然有重叠（例如 go 的 `golangci-lint` / `go-vet`、python 的 ruff/black/mypy/flake8），
 * 这是**有意允许**的重复，原因是 presentation 还包含 lint 之外的东西：
 * - `gofmt` / `rustfmt` / `dart format` 这类格式化器，不是运行时 lint 工具；
 * - `commitlint` / `editorconfig` 这类工程化工具，压根不属于运行时工具链；
 * - 模板需要给用户看的 label 与「默认勾选组合」，与分派层无关。
 * 如果为了消除重复而让模板去读 `lintTools`，反而会把「模板默认值」这类知识塞回分派层。
 *
 * 约束：本文件是**纯数据 + 纯函数**（不执行 IO、不依赖 node-bridge），
 * 因此可以被模板、bridge、测试以任意顺序静态引入。
 */

import type { ChoiceOption, ChoiceSpec } from './language-options.js';

/**
 * 重新导出选项类型，方便消费方只 import 一个子路径（`@lania-cli-v2/templates/presentation`）
 * 就能同时拿到值、类型与取值函数。
 */
export type { ChoiceOption, ChoiceSpec } from './language-options.js';

/**
 * 各运行时的模板提问选项。
 *
 * 键必须覆盖 `RUNTIME_IDS`（`node-bridge/src/plugins/toolchain.ts` 的 7 个运行时）；
 * `toolchain.test.ts` 用 `RUNTIME_IDS` 逐条校验，避免新增运行时只改一边。
 */
export const RUNTIME_PRESENTATIONS: Record<string, ChoiceSpec> = {
  node: {
    packageManagers: [
      { value: 'npm', label: 'npm' },
      { value: 'pnpm', label: 'pnpm' },
      { value: 'yarn', label: 'yarn' },
      { value: 'bun', label: 'bun' },
    ],
    defaultPackageManager: 'npm',
    lintTools: [
      { value: 'eslint', label: 'eslint' },
      { value: 'prettier', label: 'prettier' },
      { value: 'oxlint', label: 'oxlint' },
    ],
    defaultLintTools: ['eslint', 'prettier'],
    testTools: [
      { value: 'vitest', label: 'vitest' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'vitest',
  },
  bun: {
    // Bun 自带 install/test，因此默认值指向 bun 自身工具；仍允许选 vitest 兼容 Node 生态。
    packageManagers: [{ value: 'bun', label: 'bun' }],
    defaultPackageManager: 'bun',
    lintTools: [
      { value: 'eslint', label: 'eslint' },
      { value: 'oxlint', label: 'oxlint' },
    ],
    defaultLintTools: ['eslint'],
    testTools: [
      { value: 'bun-test', label: 'bun test' },
      { value: 'vitest', label: 'vitest' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'bun-test',
  },
  deno: {
    // deno 的 lint / fmt / test 都是内置实现，没有独立安装步骤。
    packageManagers: [{ value: 'deno', label: 'deno' }],
    defaultPackageManager: 'deno',
    lintTools: [
      { value: 'deno-lint', label: 'deno lint' },
      { value: 'deno-fmt', label: 'deno fmt' },
    ],
    defaultLintTools: ['deno-lint', 'deno-fmt'],
    testTools: [
      { value: 'deno-test', label: 'deno test' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'deno-test',
  },
  python: {
    packageManagers: [
      { value: 'pip', label: 'pip' },
      { value: 'uv', label: 'uv' },
      { value: 'poetry', label: 'poetry' },
    ],
    defaultPackageManager: 'pip',
    lintTools: [
      { value: 'ruff', label: 'ruff' },
      { value: 'black', label: 'black' },
      { value: 'mypy', label: 'mypy' },
      { value: 'flake8', label: 'flake8' },
    ],
    defaultLintTools: ['ruff'],
    testTools: [
      { value: 'pytest', label: 'pytest' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'pytest',
  },
  go: {
    packageManagers: [{ value: 'go', label: 'go modules' }],
    defaultPackageManager: 'go',
    lintTools: [
      { value: 'golangci-lint', label: 'golangci-lint' },
      // gofmt 是格式化器（不是 lint 工具），所以只出现在 presentation 里。
      { value: 'gofmt', label: 'gofmt' },
      { value: 'go-vet', label: 'go vet' },
    ],
    defaultLintTools: ['gofmt', 'go-vet'],
    testTools: [
      { value: 'go-test', label: 'go test' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'go-test',
  },
  rust: {
    packageManagers: [{ value: 'cargo', label: 'cargo' }],
    defaultPackageManager: 'cargo',
    lintTools: [
      { value: 'clippy', label: 'clippy' },
      // rustfmt 不是 cargo 子命令（独立二进制），因此只在 presentation 里。
      { value: 'rustfmt', label: 'rustfmt' },
    ],
    defaultLintTools: ['clippy', 'rustfmt'],
    testTools: [
      { value: 'builtin', label: 'cargo test' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'builtin',
  },
  dart: {
    packageManagers: [{ value: 'pub', label: 'dart pub' }],
    defaultPackageManager: 'pub',
    lintTools: [{ value: 'dart-analyze', label: 'dart analyze' }],
    defaultLintTools: ['dart-analyze'],
    testTools: [
      { value: 'test', label: 'package:test' },
      { value: 'skip', label: 'skip' },
    ],
    defaultTestTool: 'test',
  },
};

/**
 * 取某个运行时的模板选项（即 `RuntimeToolchain.presentation` 的值）。
 *
 * 返回的是表里的**静态对象本身**：模板侧只读，不复制（避免每 100 次 create 就多 100 份
 * 临时数组）。如果将来真需要可变副本，调用方自行 `structuredClone`。
 *
 * 未知运行时直接抛错：模板拿不到选项就意味着问答表会静默变空，宁可早失败。
 */
export function choicesFor(runtime: string): ChoiceSpec {
  const presentation = RUNTIME_PRESENTATIONS[runtime];
  if (!presentation) {
    throw new Error(`no template presentation registered for runtime "${runtime}"`);
  }
  return presentation;
}

/** 已登记 presentation 的运行时 id（顺序与定义顺序一致；测试用来对齐 `RUNTIME_IDS`）。 */
export function presentationRuntimeIds(): string[] {
  return Object.keys(RUNTIME_PRESENTATIONS);
}

// ---------------------------------------------------------------------------
// 模板族的「额外工程化工具」（B14 extras）
// ---------------------------------------------------------------------------
//
// 上面 `RUNTIME_PRESENTATIONS` 回答的是「这门语言的模板该问什么」；
// spa-react / spa-vue / toolkit / toolkit-monorepo 这类 node 模板还要问一批
// **工程化配置工具**（eslint / oxlint / prettier / oxfmt / stylelint / commitlint /
// editorconfig）—— 它们不参与 lint / build 分派（`RuntimeToolchain.lintTools` 那套
// 是给 `lan lint` 用的），只决定问哪些配置、生成哪些配置文件。
//
// 这四份列表过去在每个 `options.ts` / `questions.ts` 里各写一遍（value 顺序就是问答顺序，
// 所以顺序也是契约）。现在统一收在这里：
// - 工具目录（含 label）只定义一次；
// - 两个模板族（spa / toolkit）只是「能否选 stylelint」与默认值不同；
// - 包管理器列表顺序是 `pnpm` 优先（与 node 运行时的 `NODE_CHOICES` 不同，那里 `npm` 默认）。

/**
 * node 工程化工具的目录：`{ value, label }` 只在这里定义一次。
 *
 * 顺序有语义：它是 spa 系模板 checkbox 的展示顺序（也决定了 toolkit 去掉 stylelint 后的顺序）。
 */
const NODE_ENGINEERING_TOOLS: ChoiceOption[] = [
  { value: 'eslint', label: 'eslint' },
  { value: 'oxlint', label: 'oxlint' },
  { value: 'prettier', label: 'prettier' },
  { value: 'oxfmt', label: 'oxfmt' },
  { value: 'stylelint', label: 'stylelint' },
  { value: 'commitlint', label: 'commitlint' },
  { value: 'editorconfig', label: 'editorconfig' },
];

/** 模板问答里的包管理器顺序：`pnpm` 优先（顺序即问答顺序）。 */
const NODE_TEMPLATE_PACKAGE_MANAGERS: ChoiceOption[] = [
  { value: 'pnpm', label: 'pnpm' },
  { value: 'npm', label: 'npm' },
  { value: 'yarn', label: 'yarn' },
  { value: 'bun', label: 'bun' },
];

/** 一个模板族的工程化工具选项（问答用）。 */
export interface TemplateToolChoices {
  /** 可勾选的工程化工具；顺序即问答顺序。 */
  lintTools: ChoiceOption[];
  /**
   * `lintTools` 的 value 列表（模板 `questions.choices` 直接用它）。
   *
   * 在定义处算一次而不是让每个模板各 `map` 一次：四个模板因此共享**同一个数组实例**，
   * 「模板里再写一份字面量」会立刻被等价性测试（严格相等）抓到。
   */
  lintToolValues: string[];
  /** 预勾选的工程化工具；空数组表示不预勾（沿用调用方输入）。 */
  defaultLintTools: string[];
  /** 包管理器候选；顺序即问答顺序。 */
  packageManagers: ChoiceOption[];
  /** `packageManagers` 的 value 列表（同 `lintToolValues`，共享同一实例）。 */
  packageManagerValues: string[];
  /** 问答默认包管理器；`null` 表示不写 `default`（交给交互层取第一项）。 */
  defaultPackageManager: string | null;
}

/** 构造一个模板族选项：value 列表在这里统一从目录派生，避免多处 `map`。 */
function defineTemplateChoices(spec: {
  lintTools: ChoiceOption[];
  defaultLintTools: string[];
  packageManagers: ChoiceOption[];
  defaultPackageManager: string | null;
}): TemplateToolChoices {
  return {
    ...spec,
    lintToolValues: spec.lintTools.map((tool) => tool.value),
    packageManagerValues: spec.packageManagers.map((item) => item.value),
  };
}

/**
 * 模板族（不是运行时）的选项组。
 *
 * - `spa`：spa-react / spa-vue —— 含 `stylelint`，不预勾任何工程化工具，
 *   包管理器不写 `default`（交互层默认选第一项 `pnpm`）。
 * - `toolkit`：toolkit / toolkit-monorepo —— 不含 `stylelint`，
 *   预勾 `eslint`/`prettier`/`commitlint`/`editorconfig`，包管理器默认 `pnpm`。
 *
 * 两个族的差异是**既有行为**，集中在这里而不是散在四个 `options.ts` 里。
 */
export const NODE_TEMPLATE_CHOICES: Record<'spa' | 'toolkit', TemplateToolChoices> = {
  spa: defineTemplateChoices({
    lintTools: NODE_ENGINEERING_TOOLS,
    defaultLintTools: [],
    packageManagers: NODE_TEMPLATE_PACKAGE_MANAGERS,
    defaultPackageManager: null,
  }),
  toolkit: defineTemplateChoices({
    // 既有顺序：目录里去掉 stylelint 之后即为 toolkit 的问答顺序。
    lintTools: NODE_ENGINEERING_TOOLS.filter((tool) => tool.value !== 'stylelint'),
    defaultLintTools: ['eslint', 'prettier', 'commitlint', 'editorconfig'],
    packageManagers: NODE_TEMPLATE_PACKAGE_MANAGERS,
    defaultPackageManager: 'pnpm',
  }),
};

export type TemplateChoiceProfile = keyof typeof NODE_TEMPLATE_CHOICES;

/** 取某个模板族的工程化工具选项。 */
export function templateChoicesFor(profile: TemplateChoiceProfile): TemplateToolChoices {
  return NODE_TEMPLATE_CHOICES[profile];
}

/**
 * 会落地 git hook 的工程化工具（决定是否生成 `.husky/pre-commit` 与 `commit-msg`）。
 *
 * 与「问答里能选什么」分开，四个模板共用这一份：
 * `stylelint` 不在 toolkit 的选项里，但名单里保留它不影响行为
 * （`lintTools.includes('stylelint')` 恒为 false），改动前四个 `config.ts` 就是这么写的。
 */
export const NODE_GIT_HOOK_TOOLS: string[] = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'stylelint',
  'commitlint',
];
