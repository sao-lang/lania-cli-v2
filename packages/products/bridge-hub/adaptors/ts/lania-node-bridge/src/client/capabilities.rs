//! 把 `NodeBridgeClient` 适配成各类 bridge capability trait 的地方。
//!
//! 这些 impl 看起来重复，但它们承担了很重要的抽象边界：
//! - 上层 workflow/host 只依赖 `ConfigBridgeCapability`、`TemplateBridgeCapability` 等 trait
//! - 不直接依赖 `NodeBridgeClient` 的底层请求细节
//! - 这样未来如果 bridge 实现替换成别的后端，理论上也只需要重新实现这些 trait

use anyhow::Result;
use async_trait::async_trait;

use super::{
    AddTemplateBridgeCapability, BridgeExchange, CommitBridgeCapability, CompilerBridgeCapability,
    ConfigBridgeCapability, LintBridgeCapability, NodeBridgeClient, TemplateBridgeCapability,
};

#[async_trait(?Send)]
impl ConfigBridgeCapability for NodeBridgeClient {
    // 这里使用 `?Send`，是因为上层有些 workflow 跑在 `LocalSet` / `spawn_local` 语境里，
    // 不强制 future 为 Send，可以减少 trait 边界上的额外限制。
    async fn load_lan_config(&self, cwd: String) -> Result<BridgeExchange> {
        self.call_async(self.load_lan_config_request(cwd)).await
    }

    async fn load_tool_config(&self, cwd: String, tool: String) -> Result<BridgeExchange> {
        self.call_async(self.load_tool_config_request(cwd, tool))
            .await
    }
}

#[async_trait(?Send)]
impl TemplateBridgeCapability for NodeBridgeClient {
    // 这些实现大多只是“trait 方法 -> request 工厂 -> call_async”的薄转发。
    // 价值不在逻辑复杂，而在于把上层依赖从具体 client API 隔离开。
    async fn list_templates(&self, cwd: String) -> Result<BridgeExchange> {
        self.call_async(self.template_list_request(cwd)).await
    }

    async fn get_template_questions(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange> {
        self.call_async(self.template_questions_request(template, options))
            .await
    }

    async fn get_template_dependencies(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange> {
        self.call_async(self.template_dependencies_request(template, options))
            .await
    }

    async fn get_template_output_tasks(
        &self,
        template: String,
        options: serde_json::Value,
    ) -> Result<BridgeExchange> {
        self.call_async(self.template_output_tasks_request(template, options))
            .await
    }

    async fn render_template(
        &self,
        template: String,
        context: serde_json::Value,
        options: serde_json::Value,
    ) -> Result<BridgeExchange> {
        self.call_async(self.template_render_request(template, context, options))
            .await
    }
}

#[async_trait(?Send)]
impl AddTemplateBridgeCapability for NodeBridgeClient {
    async fn render_add_template(
        &self,
        template: String,
        context: serde_json::Value,
    ) -> Result<BridgeExchange> {
        self.call_async(self.add_template_render_request(template, context))
            .await
    }
}

#[async_trait(?Send)]
impl CompilerBridgeCapability for NodeBridgeClient {
    async fn run_dev_server(&self, cwd: String, port: Option<u16>) -> Result<BridgeExchange> {
        self.call_async(self.compiler_dev_request(cwd, port)).await
    }

    async fn run_build(
        &self,
        cwd: String,
        watch: bool,
        mode: Option<String>,
        output_dir: Option<String>,
    ) -> Result<BridgeExchange> {
        // trait 层保留 `mode/output_dir` 这些显式参数，
        // 避免上层不得不回退到“自己拼 request JSON”的低层用法。
        self.call_async(self.compiler_build_with_options_request(cwd, watch, mode, output_dir))
            .await
    }

    async fn stop_compiler(&self) -> Result<BridgeExchange> {
        self.call_async(self.compiler_stop_request()).await
    }
}

#[async_trait(?Send)]
impl LintBridgeCapability for NodeBridgeClient {
    async fn run_lint(
        &self,
        cwd: String,
        fix: bool,
        concurrency: Option<usize>,
    ) -> Result<BridgeExchange> {
        self.call_async(self.lint_run_request(cwd, fix, concurrency))
            .await
    }
}

#[async_trait(?Send)]
impl CommitBridgeCapability for NodeBridgeClient {
    async fn run_commitizen(
        &self,
        cwd: String,
        kind: String,
        scope: Option<String>,
        subject: String,
    ) -> Result<BridgeExchange> {
        self.call_async(self.commitizen_run_request(cwd, kind, scope, subject))
            .await
    }

    async fn run_commitlint(&self, cwd: String, message: String) -> Result<BridgeExchange> {
        self.call_async(self.commitlint_run_request(cwd, message))
            .await
    }
}
