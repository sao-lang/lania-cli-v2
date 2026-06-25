# `lan create` 符号索引

这页不是流程图，而是 `lan create` 链路的符号地图。目标是让你按“结构体/字段/函数”三层追代码，而不是在 `create_workflow.rs` 里来回翻。

## 1. 入口

主入口顺序：

```text
rust/crates/lania-plugins-command-create/src/lib.rs
-> CreateCommandPlugin handler
-> rust/crates/lania-workflows/src/create/create_workflow.rs
-> CreateWorkflow::run(...)
```

建议先看：

- `rust/crates/lania-plugins-command-create/src/lib.rs`
- `rust/crates/lania-workflows/src/create/create_workflow.rs`
- `rust/crates/lania-workflows/src/create/capability.rs`
- `rust/crates/lania-workflows/src/create/prompts.rs`
- `rust/crates/lania-workflows/src/workflow_hooks.rs`

## 2. 关键结构体

### `CreateWorkflowInput`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

关键字段：

- `cwd`
  - 当前工作目录，也是模板列表和目标路径推导的起点
- `path`
  - 目标路径；`"."` 会触发“当前目录必须为空”的分支
- `project_name`
  - 可显式指定；缺失时从 prompt 或目录名推导
- `template`
  - 初始模板候选；最终值仍可能由 prompt 决定
- `package_manager`
  - 影响依赖解析和安装命令构造
- `language`
  - 透传到模板上下文，供模板分支使用
- `init_git`
  - 决定是否在写盘后执行 `git init`
- `skip_install`
  - 决定是否执行依赖安装
- `dry_run`
  - 走完整渲染计划，但不写盘
- `preview`
  - 走完整渲染计划，并把文件列表回显到 notes

### `WorkflowServices`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

`create` 最常用的字段：

- `prompt`
  - 驱动 `run_create_prompt(...)` 和 `run_template_prompt(...)`
- `bridge`
  - 提供模板 bridge 能力
- `package_manager`
  - 解析包管理器和安装命令
- `exec`
  - 运行 formatter 和安装命令
- `fs`
  - 确保目录存在
- `git`
  - 初始化仓库和获取状态
- `progress`
  - 在 prompt、依赖解析、写盘期间更新进度
- `hooks`
  - 让 workflow hooks 有机会改写依赖和文件集

### `TemplateCapability<'a>`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`
- `rust/crates/lania-workflows/src/create/capability.rs`

职责：

- 把 `template.*` bridge method 包装成 Rust 侧统一能力门面
- 优先走 Node bridge
- 失败时回退到 Rust 内建声明式模板

关键方法：

- `list(...)`
- `questions(...)`
- `dependencies(...)`
- `output_tasks(...)`
- `render(...)`

### `WorkflowExecution`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

在 `create` 链里最重要的字段：

- `workflow`
  - 固定为 `"create"`
- `state`
  - `dry_run/preview` 通常为 `Planned`，真实执行为 `Completed`
- `prompts`
  - 来自脱敏后的 `prompt_state`
- `bridge_steps`
  - 记录本次 create 调用了哪些 bridge 方法
- `written_files`
  - 由 `WriteReport` 收敛而来
- `command_plans`
  - 主要是 install 命令和 git init 计划
- `notes`
  - 面向用户的人类摘要

## 3. 关键中间对象

### `prompt_state`

生成和流向：

```text
run_create_prompt(...)
-> prompt_state
-> run_template_prompt(...)
-> prompt_state.extend(...)
-> build_template_question_options(...)
-> capability.dependencies/output_tasks/render(...)
-> redact_prompt_answers(...)
-> WorkflowExecution.prompts
```

最关键的键：

- `template`
- `projectName`
- `packageManager`
- `skipInstall`
- `skipGit`
- `dryRun`
- `preview`
- `port`
- `language`
- `previewFiles`

### `template_runtime_options`

职责：

- 承接模板运行时选项，而不是原始 CLI 入参
- 是 `template.getDependencies/getOutputTasks/render` 的主参数

它会逐步被补充：

- prompt 答案
- `resolvedDependencies`
- `resolvedDevDependencies`
- `dryRun/preview`

### `files: Vec<PlannedFile>`

来源：

```text
capability.render(...)
-> rendered_files
-> map to PlannedFile
-> call_template_parse(...)
-> call_files_prepare(...)
-> write_files_with_hooks(...)
```

这里是 `create` 最重要的边界：

- 模板层只负责“给出文件计划”
- 宿主层负责“真正写盘”

## 4. 关键函数

### `CreateWorkflow::run(...)`

位置：

- `rust/crates/lania-workflows/src/create/create_workflow.rs`

职责分三段：

1. 采集输入
2. 生成计划
3. 落地执行

### `run_create_prompt(...)`

位置：

- `rust/crates/lania-workflows/src/create/prompts.rs`

职责：

- 获取模板名
- 获取项目名或目标目录输入
- 填充第一版 `prompt_state`

### `run_template_prompt(...)`

位置：

- `rust/crates/lania-workflows/src/create/prompts.rs`

职责：

- 根据模板问题补齐第二版 `prompt_state`

### `build_template_question_options(...)`

位置：

- `rust/crates/lania-workflows/src/create/prompts.rs`

职责：

- 把 CLI 输入和 prompt 状态转成给模板能力层的 options

### `resolve_package_manager(...)`

位置：

- `rust/crates/lania-workflows/src/create/helpers/package.rs`

职责：

- 决定后续依赖查询和安装命令使用哪个包管理器

### `resolve_dependency_versions(...)`

位置：

- `rust/crates/lania-workflows/src/create/helpers/package.rs`

职责：

- 并发查询依赖版本
- 产出最终 `resolvedDependencies` / `resolvedDevDependencies`

### `call_dependencies_modify(...)`

位置：

- `rust/crates/lania-workflows/src/workflow_hooks.rs`

职责：

- 给 hooks 一个机会去改写依赖集合

### `call_template_parse(...)`

位置：

- `rust/crates/lania-workflows/src/workflow_hooks.rs`

职责：

- 给 hooks 一个机会在模板渲染后改写文件列表

### `call_files_prepare(...)`

位置：

- `rust/crates/lania-workflows/src/workflow_hooks.rs`

职责：

- 在真正写盘前，对 `files` 做最后一轮 waterfall 改写

### `write_files_with_hooks(...)`

位置：

- `rust/crates/lania-workflows/src/workflow_hooks.rs`

职责：

- 逐文件写盘
- 发出 `before/after/conflict` hook 事件
- 返回 `WriteReport`

### `run_package_command(...)`

位置：

- `rust/crates/lania-workflows/src/create/helpers/package.rs`

职责：

- 真正执行依赖安装命令
- 发出 `onDependenciesInstall` 和 `onShellCommand` hook

## 5. 上下游调用关系

### 上游调用方

- `CreateCommandPlugin` handler
- `HostRuntime::execute_command()`
- `main()`

### 下游被调用方

- Node bridge `template.list`
- Node bridge `template.getQuestions`
- Node bridge `template.getDependencies`
- Node bridge `template.getOutputTasks`
- Node bridge `template.render`
- `FormatService::format_planned_files(...)`
- `write_files_with_hooks(...)`
- `run_package_command(...)`
- `services.git.init(...)`

## 6. 推荐追踪顺序

如果要在 IDE 里顺着符号查，建议按这个顺序：

1. `CreateWorkflowInput`
2. `CreateWorkflow::run`
3. `TemplateCapability`
4. `prompt_state`
5. `template_runtime_options`
6. `files`
7. `WorkflowExecution`

## 7. 检索切口

```bash
rg -n "CreateWorkflow::run|run_create_prompt|run_template_prompt|TemplateCapability|resolve_dependency_versions|write_files_with_hooks|run_package_command" rust/crates/lania-workflows/src -g '*.rs'
rg -n "template.list|template.getQuestions|template.getDependencies|template.getOutputTasks|template.render" rust ts -g '*.rs' -g '*.ts'
```
