//! create 工作流：生成项目骨架、渲染模板并规划初始化命令。
//!
//! 主要导出：new、list、questions、dependencies、output_tasks、render。

mod add_workflow;
mod capability;
mod create_workflow;
mod helpers;
mod prompts;
mod templates;

#[cfg(test)]
mod tests;
