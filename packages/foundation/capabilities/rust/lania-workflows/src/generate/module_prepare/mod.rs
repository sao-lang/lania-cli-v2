//! 模块生成前的参数归一化、目录规划与上下文准备。
//!
//! 这个目录是从原先的大文件中拆出来的“准备阶段”聚合层，只保留对外边界：
//! - `init` 负责首次初始化 `lania.module.yaml` 与默认目录骨架
//! - `planning` 负责读取配置、筛选输入、渲染生成计划
//! - `compile` 负责把单个 input 配置编译成统一的中间结构
//! - `overrides` 负责把配置中的 operations override 合并进合同 IR
//! - `source_scan` 负责按 include 规则收集源文件
//!
//! 这样调用方只依赖“准备模块生成”的两个入口函数，而不需要感知内部每一步
//! 是如何扫描文件、编译 schema、应用覆盖规则和渲染输出计划的。

mod compile;
mod init;
mod overrides;
mod planning;
mod source_scan;

pub(crate) use init::initialize_generate_module;
pub(crate) use planning::prepare_generate_module_plan;
