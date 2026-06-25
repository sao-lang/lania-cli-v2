# `lan release` 符号索引

这页专门服务于 `lan release`。这条链最容易读乱的地方，是把“计划对象”“状态快照”“最终返回结果”混在一起。本文就是把这三层拆开。

## 1. 入口

主入口顺序：

```text
rust/crates/lania-plugins-command-release/src/lib.rs
-> ReleaseCommandPlugin handler
-> rust/crates/lania-workflows/src/release/mod.rs
-> ReleaseWorkflow::run(...)
```

建议先看：

- `rust/crates/lania-plugins-command-release/src/lib.rs`
- `rust/crates/lania-workflows/src/release/mod.rs`
- `rust/crates/lania-workflows/src/release/plan.rs`
- `rust/crates/lania-workflows/src/release/state.rs`
- `rust/crates/lania-workflows/src/release/execution.rs`

## 2. 关键结构体

### `ReleaseWorkflowInput`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

关键字段：

- `cwd`
  - 所有 git、package manager、state file 解析的根目录
- `mode`
  - `Plan / Run / Resume / Status`
- `version`
  - 是否启用 version stage 的关键输入
- `profile`
  - 决定默认 verify/artifact/deploy 行为
- `env`
  - 发布环境透传
- `channel`
  - 发布渠道透传
- `from_stage` / `to_stage`
  - 控制执行范围
- `skip_stages`
  - 关闭某些 stage
- `state_file`
  - 持久化状态文件位置
- `apply`
  - 是否执行计划
- `dry_run`
  - 是否只做计划不落地
- `publish`
  - `publish_or_deploy` 阶段的重要开关
- `changelog`
  - 决定 changelog 默认行为

### `ReleasePlan`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

职责：

- 表达“应该怎么发”

关键字段：

- `cwd`
- `profile`
- `env`
- `channel`
- `version`
- `publish`
- `state_file`
- `from_stage`
- `to_stage`
- `skip_stages`
- `apply`
- `dry_run`
- `verify`
- `versioning`
- `changelog`
- `artifact`
- `deploy`
- `post_check`
- `git`
- `package_manager`

### `ReleaseStateSnapshot`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

职责：

- 表达“当前发到哪一步了”

关键字段：

- `mode`
  - 当前是 plan 还是 run
- `state_file`
  - 状态文件自身路径
- `active_range`
  - 本次真正激活的 stage 范围
- `stages`
  - 每个 stage 的状态快照
- `completed`
  - 是否整体完成
- `summary`
  - 面向用户的摘要信息

### `ReleaseStageSnapshot`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

关键字段：

- `stage`
- `status`
- `commands`
- `notes`
- `error`

### `WorkflowExecution`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

在 `release` 链里最重要的字段：

- `workflow`
  - 固定为 `"release"`
- `state`
  - `Planned / Failed / Completed`
- `written_files`
  - 一般至少包含 release state 文件
- `command_plans`
  - 展平后的 stage commands
- `git_status`
  - 当前仓库状态
- `notes`
  - 来自 state summary 和 stage summary

## 3. 关键中间对象

### `plan`

来源：

```text
build_release_plan(services, &input)
-> ReleasePlan
```

这是整个 release 链的“静态定义”：

- 哪些 stage 会启用
- 每个 stage 默认跑什么命令
- 状态文件写在哪

### `snapshot`

来源：

```text
release_state_from_plan(&plan, &status)
-> ReleaseStateSnapshot
-> write_release_state(...)
-> execute_release_plan(...)
```

这是整个 release 链的“动态状态”：

- 哪个 stage 正在运行
- 哪个 stage 已完成
- 哪个 stage 失败

### `status: GitStatus`

来源：

```text
services.git.status(&plan.cwd)
```

职责：

- 判断当前 git 仓库是否 ready
- 把分支和仓库状态写入最终工作流输出

## 4. 关键函数

### `ReleaseWorkflow::run(...)`

位置：

- `rust/crates/lania-workflows/src/release/mod.rs`

职责：

- 总编排入口
- 调 plan
- 调 state
- 决定是否执行

### `build_release_plan(...)`

位置：

- `rust/crates/lania-workflows/src/release/plan.rs`

职责：

- 把 CLI 输入和仓库上下文归一化成 `ReleasePlan`

内部最关键的 helper：

- `parse_profile(...)`
- `parse_stage(...)`
- `parse_skip_stages(...)`
- `default_verify(...)`
- `default_changelog(...)`
- `default_artifact(...)`
- `default_deploy(...)`
- `default_post_check(...)`
- `resolve_state_file(...)`

### `ordered_stages(...)`

位置：

- `rust/crates/lania-workflows/src/release/plan.rs`

职责：

- 定义 stage 的全局顺序

### `stage_selected(...)`

位置：

- `rust/crates/lania-workflows/src/release/plan.rs`

职责：

- 根据 `from_stage / to_stage / skip_stages` 过滤 stage

### `stage_enabled(...)`

位置：

- `rust/crates/lania-workflows/src/release/plan.rs`

职责：

- 判断某 stage 在当前 plan 下是否有意义

### `stage_commands(...)`

位置：

- `rust/crates/lania-workflows/src/release/plan.rs`

职责：

- 把某个 stage 展开成真正的 shell commands

### `release_state_from_plan(...)`

位置：

- `rust/crates/lania-workflows/src/release/state.rs`

职责：

- 把静态 plan 转成可持久化 snapshot

### `merge_release_state(...)`

位置：

- `rust/crates/lania-workflows/src/release/state.rs`

职责：

- `resume` 模式下复用之前已经完成的 stage

### `write_release_state(...)`

位置：

- `rust/crates/lania-workflows/src/release/state.rs`

职责：

- 把 snapshot 落盘

### `read_release_state(...)`

位置：

- `rust/crates/lania-workflows/src/release/state.rs`

职责：

- 读取既有状态文件

### `execute_release_plan(...)`

位置：

- `rust/crates/lania-workflows/src/release/execution.rs`

职责：

- 遍历 stage
- 把 stage 状态从 `Planned -> Running -> Completed/Failed`
- 在每次状态变化后写回 state 文件

### `execute_stage_commands(...)`

位置：

- `rust/crates/lania-workflows/src/release/execution.rs`

职责：

- 逐条执行 stage command
- 发 `before/after` shell hooks

### `workflow_from_state(...)`

位置：

- `rust/crates/lania-workflows/src/release/execution.rs`

职责：

- 把当前 snapshot 转成 CLI 可返回的 `WorkflowExecution`

## 5. 上下游调用关系

### 上游调用方

- `ReleaseCommandPlugin`
- `HostRuntime::execute_command()`
- `main()`

### 下游被调用方

- `build_release_plan(...)`
- `read_release_state(...)`
- `services.git.status(...)`
- `release_state_from_plan(...)`
- `merge_release_state(...)`
- `write_release_state(...)`
- `execute_release_plan(...)`
- `execute_stage_commands(...)`
- `workflow_from_state(...)`

## 6. 推荐追踪顺序

建议按下面顺序看：

1. `ReleaseWorkflowInput`
2. `ReleasePlan`
3. `ReleaseStateSnapshot`
4. `ReleaseStageSnapshot`
5. `stage_commands(...)`
6. `execute_release_plan(...)`
7. `workflow_from_state(...)`

## 7. 检索切口

```bash
rg -n "ReleaseWorkflow::run|build_release_plan|release_state_from_plan|merge_release_state|write_release_state|execute_release_plan|execute_stage_commands|workflow_from_state" rust/crates/lania-workflows/src/release -g '*.rs'
rg -n "struct ReleaseWorkflowInput|struct ReleasePlan|struct ReleaseStateSnapshot|struct ReleaseStageSnapshot" rust/crates/lania-workflows/src/models.rs -g '*.rs'
```
