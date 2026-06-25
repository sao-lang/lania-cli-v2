# `lan generate module` 符号索引

这页聚焦 `lan generate module` 的符号层，而不是流程图。重点是把“配置镜像、准备计划、写盘结果、manifest 管理”四层对象分开。

## 1. 入口

主入口顺序：

```text
rust/crates/lania-plugins-command-generate/src/lib.rs
-> GenerateCommandPlugin handler
-> rust/crates/lania-workflows/src/generate/module.rs
-> GenerateModuleWorkflow::run(...)
```

建议先看：

- `rust/crates/lania-plugins-command-generate/src/lib.rs`
- `rust/crates/lania-workflows/src/generate/module.rs`
- `rust/crates/lania-workflows/src/generate/module_prepare/planning.rs`
- `rust/crates/lania-workflows/src/generate/module_prepare/init.rs`
- `rust/crates/lania-workflows/src/generate/module_manifest.rs`
- `rust/crates/lania-workflows/src/generate/module_render.rs`
- `rust/crates/lania-workflows/src/generate/api_support/apply.rs`
- `rust/crates/lania-workflows/src/generate/types.rs`

## 2. 关键结构体

### `GenerateModuleWorkflowInput`

定义位置：

- `rust/crates/lania-workflows/src/models.rs`

关键字段：

- `cwd`
  - 所有相对路径解析的根
- `config_path`
  - `lania.module.yaml` 的位置
- `manifest_path`
  - 模块生成锁文件位置
- `input_path`
  - 限制本次只处理某部分 schema 输入
- `source_filter`
  - 按 source kind 过滤
- `target_filter`
  - 按生成目标过滤
- `entry_filter`
  - 按 entry 名过滤
- `framework`
  - 当前实现实际只支持 `lania-g`
- `main_path`
  - main.go 注入目标
- `module_name`
  - 模块名覆写
- `package_name`
  - 包名覆写
- `dry_run/check/clean/force/no_inject`
  - 决定最终是 plan、diff、校验、清理还是强制覆盖
- `mode`
  - `Apply / Plan / Diff / Init`

### `ModuleConfig`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

职责：

- `lania.module.yaml` 的 Rust 镜像

关键字段：

- `framework`
- `inputs`
- `targets`
- `output`
- `inject`
- `overrides`

### `PreparedGenerateModulePlan`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

这是整条链的中枢对象。

关键字段：

- `config_path`
- `config_dir`
- `manifest_path`
- `manifest`
- `all_entries`
  - 编译后的完整 entry 集合
- `selected_entries`
  - 本次命令真正命中的 entry 集合
- `generated_plans`
  - 本次要消费的代码生成计划
- `language`
- `framework`
- `planning_notes`

### `GeneratedContractPlan`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

关键字段：

- `path`
  - 最终输出文件路径
- `content`
  - 要写入的内容
- `owner`
  - 归属哪个 entry；`None` 通常表示共享输出

### `ModuleManifest`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

职责：

- 记录生成器认领的 managed outputs

关键字段：

- `shared_outputs`
- `entries`

### `ContractPlanSummary`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

四分法结果：

- `to_write`
- `unchanged`
- `conflicts`
- `stale`

### `ContractWriteOutcome`

定义位置：

- `rust/crates/lania-workflows/src/generate/types.rs`

职责：

- 把不同模式下的执行结果统一成一个收敛结构

## 3. 关键中间对象

### `all_entries` vs `selected_entries`

来源：

```text
compile_module_entry(...) * N
-> all_entries
-> matches_generate_module_filters(...)
-> selected_entries
```

区别：

- `all_entries`
  - 用来保持全局视图，比如 registry 文件和注入判断
- `selected_entries`
  - 只表示本次命令要落地处理的 entry

### `generated_plans`

来源：

```text
render_module_entry(...) * N
-> generated_plans
-> render_module_registry_file(...)
-> prepare_main_go_injection(...)
```

它不是写盘结果，而是“待消费的生成计划”。

### `manifest`

来源和去向：

```text
load_module_manifest(...)
-> prepared.manifest
-> update_module_manifest(...)
-> write_module_manifest(...)
```

它描述的是：

- 生成器现在认领了哪些文件
- 哪些文件是 shared outputs
- 每个 entry 的 input/IR hash 是什么

### `summary`

来源：

```text
module_manifest_managed_paths(...)
-> module_safe_overwrite_paths(...)
-> summarize_contract_plan(...)
-> summary
```

它决定最终走向：

- 只是显示计划
- 直接失败为冲突
- 真正写盘
- 顺带 clean stale 文件

## 4. 关键函数

### `GenerateModuleWorkflow::run(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module.rs`

职责：

- 分发 `init` 路径和普通路径
- 控制 `plan/diff/check/apply` 四类模式

### `initialize_generate_module(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_prepare/init.rs`

职责：

- 生成模块生成骨架
- 写出默认 `lania.module.yaml`
- 预置 `schemas/` 和 `generated/lania/` 目录

### `prepare_generate_module_plan(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_prepare/planning.rs`

职责：

- 读配置
- 校验框架
- 解析 output/manifest 路径
- 编译 entry
- 筛选 entry
- 渲染代码计划
- 准备注入计划

### `compile_module_entry(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_prepare/compile.rs`

职责：

- 把 module input 配置编译为统一 `CompiledModuleEntry`

### `render_module_entry(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_render.rs`

职责：

- 把统一 IR 渲染成实际输出计划

### `module_manifest_managed_paths(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_manifest.rs`

职责：

- 从旧 manifest 提取当前范围内的 managed paths

### `summarize_contract_plan(...)`

位置：

- `rust/crates/lania-workflows/src/generate/api_support/planning.rs`

职责：

- 计算 `to_write/unchanged/conflicts/stale`

### `apply_contract_generation(...)`

位置：

- `rust/crates/lania-workflows/src/generate/api_support/apply.rs`

职责：

- 真正把 `generated_plans` 落盘

### `update_module_manifest(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_manifest.rs`

职责：

- 更新生成器当前认领的 outputs

### `write_module_manifest(...)`

位置：

- `rust/crates/lania-workflows/src/generate/module_manifest.rs`

职责：

- 把 manifest 本身也走统一格式化和写盘管线

## 5. 上下游调用关系

### 上游调用方

- `GenerateCommandPlugin`
- `HostRuntime::execute_command()`
- `main()`

### 下游被调用方

- `initialize_generate_module(...)`
- `prepare_generate_module_plan(...)`
- `compile_module_entry(...)`
- `render_module_entry(...)`
- `prepare_main_go_injection(...)`
- `summarize_contract_plan(...)`
- `apply_contract_generation(...)`
- `remove_stale_generated_files(...)`
- `update_module_manifest(...)`
- `write_module_manifest(...)`

## 6. 推荐追踪顺序

如果要顺着符号理解，建议按下面顺序：

1. `GenerateModuleWorkflowInput`
2. `ModuleConfig`
3. `PreparedGenerateModulePlan`
4. `GeneratedContractPlan`
5. `ModuleManifest`
6. `ContractPlanSummary`
7. `ContractWriteOutcome`

## 7. 检索切口

```bash
rg -n "GenerateModuleWorkflow::run|initialize_generate_module|prepare_generate_module_plan|compile_module_entry|render_module_entry|update_module_manifest|write_module_manifest" rust/crates/lania-workflows/src -g '*.rs'
rg -n "PreparedGenerateModulePlan|GeneratedContractPlan|ModuleManifest|ContractPlanSummary|ContractWriteOutcome" rust/crates/lania-workflows/src/generate/types.rs -g '*.rs'
```
