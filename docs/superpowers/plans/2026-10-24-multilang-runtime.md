# 多语言运行时扩展 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `lan dev` / `lan build` / `lan create` / `lan add` / `lan template` 支持 web / Python / Go / Rust / Dart 五类运行时，且不改变现有前端行为。

**Architecture:** 在 Node bridge 的 `compiler` 插件内引入 `Runtime` 维度：`detectRuntime()` 探测语言，web 走现有 vite/webpack/rollup 进程内 adapter，语言走新增 spawn 型 adapter（复用 worker 隔离与 `activeCompiler.stop()`）。模板侧新增语言目录与单文件 add 资产；Rust 侧只做边界改动（config 白名单、`--runtime` 选项、构建退出码、模板名 choices）。

**Tech Stack:** TypeScript（Node 22 + tsx + `node:test`）、Rust（cargo workspace）、EJS 模板。

**Spec:** `docs/superpowers/specs/2026-10-24-multilang-runtime-design.md`

## Global Constraints

- `Runtime = 'web' | 'python' | 'go' | 'rust' | 'dart'`。`BuildTool = 'vite' | 'webpack' | 'rollup'` 仅在 web runtime 内部使用，保持不变。
- 非 web adapter 的 `result` 必须包含 `success: boolean`。
- 非 web 失败一律 fail-fast：不发 `event.dev_url` / `event.compiler_server_ready`，不伪造 `http://host:port`，不进入长任务等待 Ctrl-C。
- 构建退出码：`result.success === false` → `EXIT_RUNTIME_ERROR`；缺失 `success` → `EXIT_SUCCESS`（web 行为不变）。
- `build --watch` 对非 web runtime 直接返回失败，消息含 `watch not supported`。
- 现有 `compiler.test.ts` 的 vite/webpack/rollup 用例必须全部通过（web 行为不变）。
- 测试必须跨平台（win32 + POSIX）：禁止用 PATH 注入假二进制；用 `lan.config.runtimes.<rt>.dev/build = [process.execPath, <fixture.js>, ...]` 注入 Node fixture。
- spawn 型 adapter 复用现有 worker：`compiler-worker.ts` 按 `context.runtime` 分发；不改 `compiler-worker-client.ts` 的进程模型。
- 不做 dev 运行期实时日志流；只保证启动阶段输出。
- Rust 改动限定：`validate_lan.rs`、dev/build 的 `spec.rs`/`request.rs`/`handlers.rs`、`add` spec、`ADD_TEMPLATE_SPECS`、`add_workflow.rs`（仅新增 `componentName` 上下文）。
- 不做 runtime-aware 依赖安装；新 create 模板提供 `skipInstall` confirm 问题且默认 `true`。

## Review Focus

- 混合仓库（同时存在 `package.json` 与 `go.mod`）：探测必须按 `go.mod` > `Cargo.toml` > `pyproject.toml` > `pubspec.yaml` > `package.json` 顺序，`--runtime` / `lan.config.runtime` 覆盖一切。测试见 Task 3。
- 工具链缺失（`go`/`cargo`/`python`/`dart` 不在 PATH）：必须非零退出、不挂起、不伪造 URL。测试见 Task 5。
- dev 命令启动即失败（如 `go run .` 编译错误）：必须在就绪窗口内判定失败，不进入无限等待。测试见 Task 5。
- Ctrl-C 停止 dev：必须终止整个子进程树，不残留 `go run` 编译子进程。测试见 Task 5。
- `lan create --template go-service` 默认不得跑 npm/pnpm install。测试见 Task 1。
- Python dev 未配置 `runtimes.python.dev`：必须明确失败，不回退到伪造 server。测试见 Task 6。
- Windows 下 kill 进程树与路径分隔符：测试在 win32 跑通。测试见 Task 5。

---

### Task 1: create 的 Go 模板 + create 模板名 choices

**Files:**

- Create: `ts/packages/templates/src/templates/go-service/template.json`
- Create: `ts/packages/templates/src/templates/go-service/questions.ts`
- Create: `ts/packages/templates/src/templates/go-service/config.ts`
- Create: `ts/packages/templates/src/templates/go-service/dependencies.ts`
- Create: `ts/packages/templates/src/templates/go-service/options.ts`
- Create: `ts/packages/templates/src/templates/go-service/ejs/go.mod.ejs`
- Create: `ts/packages/templates/src/templates/go-service/ejs/main.go.ejs`
- Create: `ts/packages/templates/src/templates/go-service/ejs/lan.config.js.ejs`
- Create: `ts/packages/templates/src/templates/go-service/ejs/.gitignore.ejs`
- Create: `ts/packages/templates/src/templates/go-service/files/README.md`
- Modify: `rust/crates/lania-plugins-command-create/src/lib.rs:51-57`（`--template` choices 追加 `go-service`）
- Test: `ts/packages/templates/src/template.test.ts`
- Test: `rust/crates/lania-plugins-command-create/src/lib.rs`（`mod tests` 内新增 choices 断言）

**Interfaces:**

- Consumes: `template-runtime.ts` 的 `listRuntimeTemplates` / `getRuntimeTemplate` / `renderRuntimeTemplate` / `loadTemplateQuestions`；`questions.ts` 返回问题数组，`config.ts` 返回 `{ outputTasks, files }`，`dependencies.ts` 返回 `{ dependencies, devDependencies }`。
- Produces: 模板名 `go-service`；问题集含 `skipInstall`（`type: 'confirm'`, `default: true`）；输出文件含 `go.mod`、`main.go`。

- [ ] **Step 1: 写失败的模板测试**

在 `template.test.ts` 追加：

```ts
test('go-service template exposes files and skipInstall default', async () => {
  const names = (await listRuntimeTemplates()).map((t) => t.name);
  assert.ok(names.includes('go-service'));
  const questions = await loadTemplateQuestions('go-service', { cwd: null });
  assert.ok(questions.some((q: any) => q.name === 'skipInstall' && q.default === true));
  const rendered = await renderRuntimeTemplate('go-service', {
    cwd: null,
    context: { projectName: 'demo' },
  });
  const paths = rendered.files.map((f: any) => f.path);
  assert.ok(paths.includes('go.mod'));
  assert.ok(paths.includes('main.go'));
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `node --import tsx --test src/template.test.ts`（cwd `ts/packages/templates`）
Expected: FAIL，`go-service` 不在模板列表。

- [ ] **Step 3: 实现模板目录**

- `template.json`：`{ "name": "go-service", "schemaVersion": 1, "renderEngine": "node_bridge", "useCases": ["create"], "ownership": "node_only", "migrationLayer": "runtime_template" }`
- `options.ts`：`normalizeGoServiceOptions(options)` 返回 `{ projectName, modulePath }`，均从 `options` 兜底（`modulePath` 默认 `example.com/demo`）。
- `questions.ts`：返回 `[{ name:'projectName', type:'input' }, { name:'modulePath', type:'input', default:'example.com/demo' }, { name:'skipInstall', message:'Skip dependency install?', type:'confirm', default:true }]`，支持 `options.locale === 'zh'` 中文文案。
- `dependencies.ts`：返回 `{ dependencies: [], devDependencies: [] }`。
- `config.ts`：返回 `{ outputTasks: ['write-files'], files: [file('go.mod','ejs/go.mod.ejs'), file('main.go','ejs/main.go.ejs'), file('lan.config.js','ejs/lan.config.js.ejs'), file('.gitignore','ejs/.gitignore.ejs'), file('README.md','files/README.md')] }`。
- `ejs/go.mod.ejs`：`module <%= modulePath %>\n\ngo 1.22\n`
- `ejs/main.go.ejs`：`package main\n\nimport "fmt"\n\nfunc main() {\n\tfmt.Println("<%= projectName %>")\n}\n`
- `ejs/lan.config.js.ejs`：`module.exports = { runtime: 'go' };\n`
- `ejs/.gitignore.ejs`：`bin/\n`
- `files/README.md`：一行项目名说明。

- [ ] **Step 4: 运行模板测试确认通过**

Run: `node --import tsx --test src/template.test.ts`
Expected: PASS。

- [ ] **Step 5: 扩展 Rust create spec choices 并加测试**

在 `lib.rs` 的 `choices` 追加 `"go-service".into()`。`mod tests` 加：

```rust
#[test]
fn create_spec_accepts_go_service_template() {
    let spec = CreateCommandPlugin::spec();
    let template = spec.options.iter().find(|o| o.long == "template").unwrap();
    assert!(template.choices.iter().any(|c| c == "go-service"));
}
```

- [ ] **Step 6: 运行 Rust 测试**

Run: `cargo test --manifest-path rust/Cargo.toml -p lania-plugins-command-create`
Expected: PASS。

- [ ] **Step 7: 提交**

```bash
git add ts/packages/templates/src/templates/go-service rust/crates/lania-plugins-command-create/src/lib.rs ts/packages/templates/src/template.test.ts
git commit -m "feat: add go-service create template"
```

---

### Task 2: create 的 Rust / Python / Dart 模板

**Files:**

- Create: `ts/packages/templates/src/templates/rust-lib/{template.json,questions.ts,config.ts,dependencies.ts,options.ts,ejs/Cargo.toml.ejs,ejs/src/lib.rs.ejs,ejs/lan.config.js.ejs,ejs/.gitignore.ejs,files/README.md}`
- Create: `ts/packages/templates/src/templates/python-cli/{template.json,questions.ts,config.ts,dependencies.ts,options.ts,ejs/pyproject.toml.ejs,ejs/src/main.py.ejs,ejs/lan.config.js.ejs,ejs/.gitignore.ejs,files/README.md}`
- Create: `ts/packages/templates/src/templates/dart-cli/{template.json,questions.ts,config.ts,dependencies.ts,options.ts,ejs/pubspec.yaml.ejs,ejs/bin/main.dart.ejs,ejs/lan.config.js.ejs,ejs/.gitignore.ejs,files/README.md}`
- Modify: `rust/crates/lania-plugins-command-create/src/lib.rs`（choices 追加三个名字）
- Test: `ts/packages/templates/src/template.test.ts`

**Interfaces:**

- Consumes: 与 Task 1 相同的模板运行时。
- Produces: 模板名 `rust-lib`、`python-cli`、`dart-cli`；每个都含 `skipInstall` 默认 `true`；输出分别含 `Cargo.toml`、`pyproject.toml`、`pubspec.yaml`。

- [ ] **Step 1: 写失败测试**

```ts
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
    const paths = rendered.files.map((f: any) => f.path);
    assert.ok(paths.includes(manifest), `${name} missing ${manifest}`);
    const questions = await loadTemplateQuestions(name, { cwd: null });
    assert.ok(questions.some((q: any) => q.name === 'skipInstall' && q.default === true));
  }
});
```

- [ ] **Step 2: 运行确认失败**

Run: `node --import tsx --test src/template.test.ts`
Expected: FAIL。

- [ ] **Step 3: 实现三个模板目录**

按 Task 1 的结构实现；每个模板：

- `lan.config.js.ejs` 写对应 `runtime: '<lang>'`。
- `rust-lib` 输出 `Cargo.toml`、`src/lib.rs`、`.gitignore`（`target/`）。
- `python-cli` 输出 `pyproject.toml`、`src/main.py`、`.gitignore`（`dist/`、`__pycache__/`）。
- `dart-cli` 输出 `pubspec.yaml`、`bin/main.dart`、`.gitignore`（`.dart_tool/`）。
- 每个 `questions.ts` 含 `projectName`（input）、`skipInstall`（confirm，default true）。

- [ ] **Step 4: 运行确认通过**

Run: `node --import tsx --test src/template.test.ts`
Expected: PASS。

- [ ] **Step 5: 扩展 Rust choices**

在 create spec choices 追加 `"rust-lib"`、`"python-cli"`、`"dart-cli"`。Task 1 的 choices 测试改为断言四个新模板名都在。

- [ ] **Step 6: 运行 Rust 测试并提交**

Run: `cargo test --manifest-path rust/Cargo.toml -p lania-plugins-command-create`
Expected: PASS。

```bash
git add ts/packages/templates/src/templates rust/crates/lania-plugins-command-create/src/lib.rs ts/packages/templates/src/template.test.ts
git commit -m "feat: add rust python dart create templates"
```

---

### Task 3: `Runtime` 类型与 `detectRuntime()`

**Files:**

- Modify: `ts/packages/node-bridge/src/plugins/compiler-shared.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-detect.test.ts`

**Interfaces:**

- Produces:

```ts
export type Runtime = 'web' | 'python' | 'go' | 'rust' | 'dart';
export const RUNTIMES: readonly Runtime[];
export function isRuntime(value: unknown): value is Runtime;
export async function detectRuntime(
  config: Record<string, unknown>,
  cwd: string,
  override?: unknown,
): Promise<Runtime>;
```

- `CompilerAdapterContext` 增加 `runtime: Runtime;`
- 把 `ActiveCompiler.tool`、`createCompilerResult` / `buildResult` / `fallbackDevResult` / `compilerStartEvent` / `compilerStatusEvent` / `compilerServerReadyEvent` / `compilerAssetEvent` / `compilerIssueEvent` / `compilerWatchChangeEvent` / `compilerDoneEvent` 的 `tool: BuildTool` 参数类型放宽为 `tool: Runtime`。`CompilerAdapter.tool` 仍为 `BuildTool`，`detectBuildTool()` 仍返回 `BuildTool`。

- [ ] **Step 1: 写失败测试**

`runtime-detect.test.ts`（`node:test` + `mkdtemp`）：

```ts
const cases: Array<[string[], Record<string, unknown>, unknown, Runtime]> = [
  [['go.mod'], {}, 'rust', 'rust'], // override 最高
  [['package.json'], { runtime: 'go' }, undefined, 'go'], // 配置优先
  [['go.mod'], {}, undefined, 'go'],
  [['Cargo.toml'], {}, undefined, 'rust'],
  [['pyproject.toml'], {}, undefined, 'python'],
  [['pubspec.yaml'], {}, undefined, 'dart'],
  [['package.json'], {}, undefined, 'web'],
  [['go.mod', 'package.json'], {}, undefined, 'go'], // 强信号优先
  [[], {}, undefined, 'web'], // 兜底
];
```

并断言 `isRuntime('go') === true`、`isRuntime('ruby') === false`。

- [ ] **Step 2: 运行确认失败**

Run: `node --import tsx --test src/plugins/runtime-detect.test.ts`（cwd `ts/packages/node-bridge`）
Expected: FAIL，`detectRuntime` 未导出。

- [ ] **Step 3: 实现类型与探测**

在 `compiler-shared.ts`：

- 定义 `Runtime` / `RUNTIMES` / `isRuntime`。
- 特征文件常量顺序：`[['go.mod','go'],['Cargo.toml','rust'],['pyproject.toml','python'],['pubspec.yaml','dart'],['package.json','web']]`。
- `detectRuntime`：先校验 `override`（`isRuntime`），再校验 `config.runtime`（`isRuntime` 且不为 `'web'` 时也直接采用，`'web'` 也直接采用），再遍历特征文件用 `fs.access`（`node:fs/promises`）判断存在，最后返回 `'web'`。
- 按上文放宽事件/结果构造函数的 `tool` 类型。

- [ ] **Step 4: 运行测试确认通过**

Run: `node --import tsx --test src/plugins/runtime-detect.test.ts`
Expected: PASS。

- [ ] **Step 5: 跑 typecheck 与现有回归**

Run: `pnpm -C ts typecheck && pnpm -C ts/packages/node-bridge test`
Expected: PASS（web 测试不回归）。

- [ ] **Step 6: 提交**

```bash
git add ts/packages/node-bridge/src/plugins/compiler-shared.ts ts/packages/node-bridge/src/plugins/runtime-detect.test.ts
git commit -m "feat: add runtime detection"
```

---

### Task 4: Rust 边界改动（config 白名单 / `--runtime` / 构建退出码）

**Files:**

- Modify: `rust/crates/lania-config/src/validate_lan.rs`（`allowed` 数组加 `"runtime"`、`"runtimes"`）
- Modify: `rust/crates/lania-config/src/tests.rs`
- Modify: `rust/crates/lania-plugins-command-dev/src/spec.rs`
- Modify: `rust/crates/lania-plugins-command-dev/src/request.rs`
- Modify: `rust/crates/lania-plugins-command-build/src/spec.rs`
- Modify: `rust/crates/lania-plugins-command-build/src/request.rs`
- Modify: `rust/crates/lania-plugins-command-build/src/handlers.rs`
- Test: dev/build crate 的 `mod tests`

**Interfaces:**

- Consumes: `CommandContext.argv.options.get("runtime")`；`BridgeCommandRun.exchange.response.result`。
- Produces: `compiler.dev` / `compiler.build` params 含 `runtime: string | null`；`fn resolve_build_exit_code(result: Option<&serde_json::Value>) -> i32`。
- 注意：`dev/lib.rs` 的 `registers_dev_command_spec` 断言 `dev_spec.options.len() == 7`，新增 `--runtime` 后改为 `8`，计划内必须同步修改该断言。

- [ ] **Step 1: 写失败的 Rust 测试**

`validate_lan.rs` 测试：

```rust
#[test]
fn accepts_runtime_config() {
    let errors = validate_lan_config(&serde_json::json!({
        "version": 1,
        "runtime": "go",
        "runtimes": { "go": { "dev": ["go", "run", "."] } }
    }), 1);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}
```

dev `lib.rs` 测试（仿现有 `builds_dev_bridge_request_from_context`：同样的 `CommandContext` 构造 + `NodeBridgeClient::new(BridgeClientConfig::default())`，options 追加 `"runtime": "go"`）：

```rust
#[test]
fn dev_request_forwards_runtime() {
    let context = CommandContext { /* 同 builds_dev_bridge_request_from_context，options 加 ("runtime".into(), json!("go")) */ };
    let bridge = NodeBridgeClient::new(BridgeClientConfig::default());
    let request = DevCommandPlugin::build_request(&context, &bridge);
    assert_eq!(request.params["runtime"], serde_json::json!("go"));
}
```

build `lib.rs` 测试：同构加 `--runtime`；另加：

```rust
#[test]
fn build_exit_code_maps_failure() {
    assert_eq!(resolve_build_exit_code(Some(&serde_json::json!({"success": false}))), EXIT_RUNTIME_ERROR);
    assert_eq!(resolve_build_exit_code(Some(&serde_json::json!({"success": true}))), EXIT_SUCCESS);
    assert_eq!(resolve_build_exit_code(Some(&serde_json::json!({}))), EXIT_SUCCESS);
    assert_eq!(resolve_build_exit_code(None), EXIT_SUCCESS);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path rust/Cargo.toml -p lania-config -p lania-plugins-command-dev -p lania-plugins-command-build`
Expected: FAIL。

- [ ] **Step 3: 实现**

- `validate_lan.rs`：`allowed` 加 `"runtime"`、`"runtimes"`。
- dev spec：加 `OptionSpec { long: "runtime", short: None, help: "Override runtime (web|python|go|rust|dart)", value_kind: String, choices: ["web","python","go","rust","dart"], ... }`；同步把 `registers_dev_command_spec` 的 `options.len()` 断言从 `7` 改为 `8`。
- dev request：`request.params["runtime"] = json!(runtime_option)`（在现有 params 赋值旁）。
- build spec：同 dev 的 `--runtime`。
- build request：`request.params["runtime"] = json!(runtime_option)`。
- build handlers：

```rust
pub(crate) fn resolve_build_exit_code(result: Option<&serde_json::Value>) -> i32 {
    let success = result
        .and_then(|value| value.get("success"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if success { lania_host::EXIT_SUCCESS } else { lania_host::EXIT_RUNTIME_ERROR }
}
```

`BuildCommandHandler::execute` 改为读取 `run.exchange.response.result` 后调用该函数并传给 `complete_bridge`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test --manifest-path rust/Cargo.toml -p lania-config -p lania-plugins-command-dev -p lania-plugins-command-build`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add rust/crates/lania-config rust/crates/lania-plugins-command-dev rust/crates/lania-plugins-command-build
git commit -m "feat: add runtime config, --runtime flag, and build exit code"
```

---

### Task 5: spawn adapter 核心 + Go adapter + worker/compiler 接入

**Files:**

- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/types.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/commands.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/spawn.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/go.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/index.ts`
- Modify: `ts/packages/node-bridge/src/plugins/compiler.ts`
- Modify: `ts/packages/node-bridge/src/workers/compiler-worker.ts`
- Modify: `ts/packages/node-bridge/src/plugins/compiler-worker-protocol.ts`
- Modify: `ts/packages/node-bridge/src/plugins/compiler-worker-client.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters.test.ts`

**Interfaces:**

- Produces:

```ts
// types.ts
export type SpawnRuntime = Exclude<Runtime, 'web'>;
export interface RuntimeAdapter {
  runtime: SpawnRuntime;
  handleDev(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
  ): Promise<CompilerExecution | null>;
  handleBuild(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
  ): Promise<CompilerExecution | null>;
}

// commands.ts
export interface SpawnCommand {
  command: string;
  args: string[];
  env: Record<string, string>;
}
export function resolveSpawnCommand(
  runtime: SpawnRuntime,
  action: 'dev' | 'build',
  context: CompilerAdapterContext,
  params: CompilerAdapterParams,
): SpawnCommand | null;

// spawn.ts
export async function runSpawnBuild(
  runtime: SpawnRuntime,
  command: SpawnCommand,
  cwd: string,
): Promise<CompilerExecution>;
export async function startSpawnDev(
  runtime: SpawnRuntime,
  command: SpawnCommand,
  cwd: string,
  params: CompilerAdapterParams,
): Promise<CompilerExecution>;
export async function killProcessTree(child: ChildProcess): Promise<void>;
export function failureExecution(
  runtime: SpawnRuntime,
  action: 'dev' | 'build',
  message: string,
): CompilerExecution;

// index.ts
export function getRuntimeAdapters(): RuntimeAdapter[];
export function resolveRuntimeAdapter(runtime: Runtime): RuntimeAdapter | null;
```

- 默认命令表：

```ts
const DEFAULT_COMMANDS: Record<SpawnRuntime, { dev?: string[]; build?: string[] }> = {
  go: { dev: ['go', 'run', '.'], build: ['go', 'build', '-o', '{output}', '.'] },
  rust: { dev: ['cargo', 'run'], build: ['cargo', 'build', '--release'] },
  python: { build: ['python', '-m', 'build'] },
  dart: { dev: ['dart', 'run'], build: ['dart', 'compile', 'exe', 'bin/main.dart'] },
};
```

- env：dev 注入 `PORT`/`HOST`；build 注入 `OUTPUT_DIR`。`{output}` 替换为 `params.outputDir ?? 'bin/app'`。

- [ ] **Step 1: 写失败的 adapter 测试**

`runtime-adapters.test.ts` 用 `mkdtemp` 写 fixture JS：

```ts
async function writeFixture(dir: string, name: string, body: string) {
  const file = path.join(dir, name);
  await writeFile(file, body, 'utf8');
  return file;
}
// fixture: 打印 argv/env，按 process.argv.includes('--fail') 退出 1
```

用例：

- `resolveSpawnCommand`：go build 默认 → `['go','build','-o',<output>,'.']`；配置覆盖 `runtimes.go.build=['node','x.js']` → 精确匹配；`{output}` 替换为 `params.outputDir`。
- go build 成功：配置为 `[process.execPath, fixture]`，fixture exit 0 → `result.success === true`，事件含 `compiler_start` 与 `compiler_done(success:true)`，`result.runtime === 'go'`。
- go build 失败：fixture `--fail` → `success === false`、`compiler_issue` severity `error`、无 `activeCompiler`。
- 工具链缺失：命令配置为 `__lania_missing_bin__` → `success === false`，不抛未捕获异常。
- `build --watch`：`params.watch === true` → `success === false`，事件消息含 `watch not supported`。
- dev 长任务：dev fixture 打印 `ready` 后 `setInterval` → 返回 `activeCompiler`；`await activeCompiler.stop()` 后进程退出。
- env 注入：dev fixture 把 `process.env.PORT`/`HOST` 写入文件 → 断言值等于 `params.port`/`params.host`。
- 注册表：`getRuntimeAdapters()` 含 `go`；`resolveRuntimeAdapter('web') === null`。

- [ ] **Step 2: 运行确认失败**

Run: `node --import tsx --test src/plugins/runtime-adapters.test.ts`（cwd `ts/packages/node-bridge`）
Expected: FAIL。

- [ ] **Step 3: 实现 commands.ts**

- 读 `context.lanConfig.runtimes`（`asRecord` 后取 `[runtime][action]`）；值为 `string[]` 时使用。
- 否则用 `DEFAULT_COMMANDS`；python dev 无默认 → 返回 `null`。
- `{output}` 替换；构造 env。

- [ ] **Step 4: 实现 spawn.ts**

- 用 `node:child_process.spawn(command, args, { cwd, env: { ...process.env, ...command.env }, stdio: ['ignore','pipe','pipe'] })`。
- 行分割 stdout/stderr；stdout → `logEvent`(info)，stderr → `compilerIssueEvent`(warning)。
- `runSpawnBuild`：等待 `close`；`success = exitCode === 0`；失败时补 `compilerIssueEvent(runtime,'error', stderr 尾段)`；发 `compilerStart`/`compilerDone`；返回 `{ result: { ...createCompilerResult(runtime,'build',...), success, runtime }, events }`。
- `startSpawnDev`：spawn 后竞速 `close` / 首个输出 / 1000ms 超时，三种结果：
  - `close` 先到且 `exitCode !== 0` → `failureExecution(runtime, 'dev', \`dev command exited before ready (code ${exitCode})\`)`。
  - `close` 先到且 `exitCode === 0` → 视为一次性 dev 命令成功：`result = { ...createCompilerResult(runtime,'dev','runtime',{}), success:true, longRunning:false }`，不返回 `activeCompiler`。
  - 首个输出先到，或 1000ms 超时 → 返回带 `activeCompiler = { tool: runtime, action:'dev', stop: () => killProcessTree(child) }` 的 `CompilerExecution`，`result.success = true`、`longRunning = true`。
  - 启动错误（`child.on('error')`/ENOENT）→ `failureExecution(runtime, 'dev', 'command not found: <command>')`。
- `killProcessTree`：win32 用 `spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'])`；其它平台 `child.kill('SIGTERM')` 并在 2s 后 `SIGKILL` 兜底。

- [ ] **Step 5: 实现 go.ts**

```ts
export const goRuntimeAdapter: RuntimeAdapter = {
  runtime: 'go',
  async handleDev(params, context) {
    const command = resolveSpawnCommand('go', 'dev', context, params);
    return command
      ? startSpawnDev('go', command, params.cwd, params)
      : failureExecution('go', 'dev', 'go dev command is not configured');
  },
  async handleBuild(params, context) {
    if (params.watch) return failureExecution('go', 'build', 'watch not supported for runtime go');
    const command = resolveSpawnCommand('go', 'build', context, params);
    return command
      ? runSpawnBuild('go', command, params.cwd)
      : failureExecution('go', 'build', 'go build command is not configured');
  },
};
```

`failureExecution(runtime, action, message)` 放在 `spawn.ts` 导出：返回 `{ result: { ...createCompilerResult(runtime, action, 'fallback', {}), success:false, error: message }, events: [compilerIssueEvent(runtime,'error',message), compilerDoneEvent(runtime, action, false, 'fallback', {})] }`。

- [ ] **Step 6: 实现 index.ts**

注册 `goRuntimeAdapter`；`resolveRuntimeAdapter` 对 `'web'` 返回 `null`。

- [ ] **Step 7: 接入 worker 分发**

`compiler-worker.ts`：

```ts
const adapter =
  message.context.runtime === 'web'
    ? resolveCompilerAdapter(message.context.tool)
    : resolveRuntimeAdapter(message.context.runtime);
```

`createFallbackExecution` 的 `tool` 字段改用 `message.context.runtime`。
`compiler-worker-protocol.ts`：`result.tool` 类型改 `Runtime`；`stopped.tool` 改 `Runtime | null`。
`compiler-worker-client.ts`：`attachStreamCapture` / `workerOutputEvents` 的 `tool` 参数改 `Runtime`。

- [ ] **Step 8: 接入 compiler.ts 并保留 web 路径**

- `handleDev`：`const runtime = await detectRuntime(lanConfig.config, cwd, params.runtime);`；context 增加 `runtime`；`runtime === 'web'` 时保持原有 `resolveRuntimeWarning` 与 `fallbackDevResult`；非 web 时 `execution ?? failureExecution(runtime,'dev','runtime adapter unavailable')`。全局 catch 里：`'web'` 仍返回 `fallbackDevResult('vite', ...)`，非 web 返回 `failureExecution(...)`。
- `handleBuild`：同构；非 web 的 `execution ?? failureExecution(runtime,'build','runtime adapter unavailable')`；catch 分支按 `runtime` 区分。
- `createAdapterParams` 传入 `runtime`（从 params 或已探测结果）。

- [ ] **Step 9: 运行 adapter 测试 + web 回归 + typecheck**

Run: `node --import tsx --test src/plugins/runtime-adapters.test.ts src/plugins/compiler.test.ts`
Run: `pnpm -C ts typecheck`
Expected: PASS。

- [ ] **Step 10: 提交**

```bash
git add ts/packages/node-bridge/src/plugins/runtime-adapters* ts/packages/node-bridge/src/plugins/compiler.ts ts/packages/node-bridge/src/plugins/compiler-worker-*.ts ts/packages/node-bridge/src/workers/compiler-worker.ts
git commit -m "feat: add spawn runtime adapter pipeline with go"
```

---

### Task 6: Python / Rust / Dart adapter

**Files:**

- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/python.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/rust.ts`
- Create: `ts/packages/node-bridge/src/plugins/runtime-adapters/dart.ts`
- Modify: `ts/packages/node-bridge/src/plugins/runtime-adapters/index.ts`
- Modify: `ts/packages/node-bridge/src/plugins/runtime-adapters.test.ts`

**Interfaces:**

- Produces: `pythonRuntimeAdapter`、`rustRuntimeAdapter`、`dartRuntimeAdapter`，均实现 `RuntimeAdapter`；`getRuntimeAdapters()` 返回四个 runtime。

- [ ] **Step 1: 写失败测试**

追加用例：

- python：`runtimes.python.build = [node, fixture]` → build 成功；**无** `runtimes.python.dev` 时 `handleDev` → `success === false` 且消息含 `dev command is not configured`。
- rust：默认 build 命令为 `['cargo','build','--release']`（断言 `resolveSpawnCommand`）。
- dart：默认 dev 为 `['dart','run']`，build 为 `['dart','compile','exe','bin/main.dart']`。
- `getRuntimeAdapters().map(a => a.runtime).sort()` 等于 `['dart','go','python','rust']`。

- [ ] **Step 2: 运行确认失败**

Run: `node --import tsx --test src/plugins/runtime-adapters.test.ts`
Expected: FAIL。

- [ ] **Step 3: 实现三个 adapter**

三个文件结构同 `go.ts`：`handleDev` 用 `resolveSpawnCommand`，`null` → `failureExecution(...'dev command is not configured')`；`handleBuild` 先 `if (params.watch) return failureExecution(...,'watch not supported for runtime <rt>')`，再 resolve + 执行。在 `index.ts` 注册。

- [ ] **Step 4: 运行确认通过**

Run: `node --import tsx --test src/plugins/runtime-adapters.test.ts src/plugins/compiler.test.ts`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add ts/packages/node-bridge/src/plugins/runtime-adapters*
git commit -m "feat: add python rust dart runtime adapters"
```

---

### Task 7: `lan add` 单文件语言模板

**Files:**

- Create: `ts/packages/templates/src/add-templates/assets/go-handler.ejs`
- Create: `ts/packages/templates/src/add-templates/assets/python-module.ejs`
- Create: `ts/packages/templates/src/add-templates/assets/rust-module.ejs`
- Create: `ts/packages/templates/src/add-templates/assets/dart-class.ejs`
- Modify: `ts/packages/templates/src/add-template-runtime.ts`（`ADD_TEMPLATES` 加四项；`renderAddRuntimeTemplate` 增加 `componentName` 变量）
- Modify: `rust/crates/lania-plugins-command-add/src/lib.rs`（`--template` choices）
- Modify: `rust/crates/lania-workflows/src/create/templates.rs`（`ADD_TEMPLATE_SPECS`）
- Modify: `rust/crates/lania-workflows/src/create/add_workflow.rs`（上下文加 `componentName`）
- Test: `ts/packages/templates/src/template.test.ts`
- Test: `rust/crates/lania-plugins-command-add/src/lib.rs`

**Interfaces:**

- Consumes: `renderAddRuntimeTemplate(name, { context })`。
- Produces: add 模板名 `go-handler`（extname `go`）、`python-module`（`py`）、`rust-module`（`rs`）、`dart-class`（`dart`）；EJS 变量 `componentName`。

- [ ] **Step 1: 写失败测试**

TS：

```ts
test('language add templates render component name', async () => {
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
    assert.ok(rendered.content.includes('Widget'));
  }
});
```

Rust：

```rust
#[test]
fn add_spec_lists_language_templates() {
    let spec = AddCommandPlugin::spec();
    let template = spec.options.iter().find(|o| o.long == "template").unwrap();
    for name in ["go-handler", "python-module", "rust-module", "dart-class"] {
        assert!(template.choices.iter().any(|c| c == name));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `node --import tsx --test src/template.test.ts`（templates 包）
Run: `cargo test --manifest-path rust/Cargo.toml -p lania-plugins-command-add`
Expected: FAIL。

- [ ] **Step 3: 实现资产与 manifest**

`ADD_TEMPLATES` 追加：

```ts
{ name: 'go-handler', label: 'Go handler', extname: 'go', schemaVersion: 1 },
{ name: 'python-module', label: 'Python module', extname: 'py', schemaVersion: 1 },
{ name: 'rust-module', label: 'Rust module', extname: 'rs', schemaVersion: 1 },
{ name: 'dart-class', label: 'Dart class', extname: 'dart', schemaVersion: 1 },
```

`renderAddRuntimeTemplate` 的 EJS locals 增加：

```ts
componentName:
  typeof context.componentName === 'string'
    ? context.componentName
    : typeof context.projectName === 'string'
      ? context.projectName
      : 'lania-app',
```

四个 ejs 使用 `<%= componentName %>`（例如 `func <%= componentName %>() {}`、`class <%= componentName %> {}`）。

- [ ] **Step 4: 扩展 Rust choices 与 SPECS**

- `add/lib.rs` choices 追加四个名字。
- `ADD_TEMPLATE_SPECS` 追加对应 `AddTemplateSpec { name, label }`。
- `add_workflow.rs` 的 `json!({...})` 增加 `"componentName": prompt_state.get("name").cloned().unwrap_or(Value::Null)`（先算 `let component_name = ...;` 再传入，避免借用冲突）。

- [ ] **Step 5: 运行确认通过**

Run: `node --import tsx --test src/template.test.ts`
Run: `cargo test --manifest-path rust/Cargo.toml -p lania-plugins-command-add -p lania-workflows`
Expected: PASS。

- [ ] **Step 6: 提交**

```bash
git add ts/packages/templates/src/add-templates ts/packages/templates/src/add-template-runtime.ts rust/crates/lania-plugins-command-add rust/crates/lania-workflows/src/create
git commit -m "feat: add language single-file add templates"
```

---

### Task 8: 文档与全量验证

**Files:**

- Modify: `README.zh-CN.md`（新增"多语言运行时"小节：`runtime` 探测顺序、`lan.config.runtime/runtimes` 示例、`--runtime`、各语言模板名、`--skip-install`）
- Modify: `docs-site/guide/` 下对应指南（若存在命令参考页，追加 `--runtime` 与 `runtimes` 配置）

**Interfaces:**

- Consumes: Task 1–7 的最终行为。

- [ ] **Step 1: 写文档**

README 增加：

```md
## 多语言运行时

`lan dev` / `lan build` 按项目语言分发工具链。探测顺序：`--runtime` > `lan.config.runtime` > 特征文件（go.mod / Cargo.toml / pyproject.toml / pubspec.yaml / package.json）> web。

\`\`\`js
module.exports = {
runtime: 'go',
runtimes: {
go: { dev: ['go', 'run', '.'], build: ['go', 'build', '-o', 'bin/app', '.'] },
},
};
\`\`\`

非 web runtime 构建失败会以非零退出码返回；`build --watch` 暂不支持非 web runtime。
可用模板：`go-service`、`rust-lib`、`python-cli`、`dart-cli`（create）；`go-handler`、`python-module`、`rust-module`、`dart-class`（add）。
\`\`\`bash
lan create --template go-service --skip-install
\`\`\`
```

- [ ] **Step 2: 全量 TS 测试与类型检查**

Run: `pnpm -C ts test && pnpm -C ts typecheck`
Expected: PASS。

- [ ] **Step 3: 全量 Rust 测试与 lint**

Run: `cargo test --manifest-path rust/Cargo.toml --workspace`
Run: `cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets -- -D warnings`
Expected: PASS。

- [ ] **Step 4: 真实工具链冒烟（本机有则跑）**

```bash
mkdir -p /tmp/lania-go-smoke && cd /tmp/lania-go-smoke
printf 'module smoke\n\ngo 1.22\n' > go.mod
printf 'package main\nfunc main(){}\n' > main.go
lan build     # 期望退出码 0
printf 'package main\nfunc main(){ broken }\n' > main.go
lan build     # 期望退出码非 0
```

Rust/Python/Dart 同理（本机有对应工具链时）；无工具链则记录"未执行，环境缺失"。

- [ ] **Step 5: 提交**

```bash
git add README.zh-CN.md docs-site
git commit -m "docs: document multi-language runtime support"
```
