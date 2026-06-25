use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use crate::{
    execution::{CommandExecution, CommandExecutionContext, CommandHandler},
    plugin::{LifecyclePhase, Plugin, PluginKind, PluginMeta, PluginSetupContext},
    runtime::Host,
    HostRuntime,
};

// 说明：
// - 这些测试里有少量用例会通过 `std::env::set_var/remove_var` 修改环境变量（例如 LANIA_PRODUCT_ROOT）。
// - Rust 的测试默认是并行执行的；进程级环境变量是全局共享的，这会导致偶发竞态（flaky）。
// - 因此这里用一个全局 Mutex 把“依赖环境变量的测试片段”串行化，保证稳定性。
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("lania-host-{name}-{unique}"));
    fs::create_dir_all(&path).expect("temp dir created");
    path
}

struct LifecyclePlugin;

struct LifecycleHandler;

impl Plugin for LifecyclePlugin {
    fn meta(&self) -> PluginMeta {
        PluginMeta {
            name: "lifecycle".into(),
            version: "0.1.0".into(),
            kind: PluginKind::Rust,
            requires: vec![],
            before: vec![],
            after: vec![],
        }
    }

    fn setup(&self, ctx: &mut PluginSetupContext<'_>) -> Result<()> {
        ctx.commands.register(lania_command::CommandSpec::new(
            "lifecycle",
            "lifecycle command",
            "lifecycle",
        ))?;
        ctx.handlers
            .register("lifecycle", Box::new(LifecycleHandler))?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl CommandHandler for LifecycleHandler {
    async fn execute(&self, ctx: &CommandExecutionContext<'_>) -> Result<CommandExecution> {
        Ok(ctx.complete_workflow(
            lania_workflows::WorkflowExecution {
                workflow: "lifecycle".into(),
                state: lania_workflows::WorkflowState::Completed,
                target_dir: ctx.command().cwd.clone(),
                prompts: Default::default(),
                bridge_steps: vec![],
                written_files: vec![],
                conflicts: vec![],
                command_plans: vec![],
                git_status: None,
                interactive_rendered: false,
                notes: vec![],
            },
            0,
        ))
    }
}

#[tokio::test]
async fn records_plugin_lifecycle_phases() {
    let mut host = HostRuntime::new();
    host.register_plugin(Box::new(LifecyclePlugin))
        .expect("plugin registers");

    host.initialize().await.expect("host initializes");
    host.execute_command(&lania_command::CommandContext {
        cwd: "/repo".into(),
        argv: lania_command::ParsedArgv::default(),
        handler_id: "lifecycle".into(),
        trace_id: "trace-runtime".into(),
    })
    .await
    .expect("command executes");
    host.shutdown_async().await.expect("host shuts down");

    assert_eq!(
        host.lifecycle_phases(),
        vec![
            LifecyclePhase::Discover,
            LifecyclePhase::Resolve,
            LifecyclePhase::Load,
            LifecyclePhase::Setup,
            LifecyclePhase::RuntimeStart,
            LifecyclePhase::CommandExecute,
            LifecyclePhase::Shutdown,
        ]
    );
}

#[test]
fn discovers_project_node_plugins_from_lan_config() {
    let host = HostRuntime::new();
    let root = temp_dir("project-plugins");
    fs::write(
        root.join("lan.config.js"),
        "export default { plugins: ['@demo/project-plugin'] };\n",
    )
    .expect("lan config written");

    let plugins = host.discover_project_node_plugins_from_cwd(root.display().to_string());
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].package, "@demo/project-plugin");

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn loads_project_lan_config_snapshot() {
    let host = HostRuntime::new();
    let root = temp_dir("project-config");
    fs::write(
        root.join("lan.config.js"),
        "export default { buildTool: 'webpack', lintTools: ['eslint'], plugins: ['@demo/project-plugin'] };\n",
    )
    .expect("lan config written");

    let snapshot = host
        .load_lan_config_snapshot_from_cwd_async(root.display().to_string())
        .await
        .expect("config snapshot loads");

    assert_eq!(snapshot.build_tool, "webpack");
    assert_eq!(snapshot.lint_tools, vec!["eslint"]);
    assert_eq!(snapshot.plugins.len(), 1);

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn bootstraps_dynamic_commands_before_cli_build() {
    let mut host = HostRuntime::new();
    let root = temp_dir("dynamic-commands");
    fs::write(
        root.join("lan.config.js"),
        "export default { extensions: { dynamicCommands: true }, plugins: ['./lania.plugin.js'] };\n",
    )
    .expect("lan config written");
    fs::write(
        root.join("lania.schemas.js"),
        "export default { runtimeCommands: [{ mount: 'ops', commands: [{ name: 'ping', handler: { plugin: './lania.plugin.js', method: 'ops.ping' } }] }] };\n",
    )
    .expect("runtime manifest written");
    fs::write(
        root.join("lania.plugin.js"),
        r#"export default {
  name: "demo-dynamic",
  methods: ["ops.ping"],
  async handle(method, params) {
if (method !== "ops.ping") return null;
return { result: { ok: true, exitCode: 0 }, events: [] };
  }
};
"#,
    )
    .expect("plugin file written");

    host.initialize().await.expect("host initializes");
    let summary = host
        .bootstrap_project_extensions_from_cwd_async(root.display().to_string())
        .await
        .expect("dynamic commands bootstrap");

    assert_eq!(summary.dynamic_commands, 1);
    assert!(host
        .command_specs()
        .iter()
        .any(|command| command.name == "ops"));

    let execution = host
        .execute_command(&lania_command::CommandContext {
            cwd: root.display().to_string(),
            argv: lania_command::ParsedArgv::default(),
            handler_id: "dynamic.mount.ops".into(),
            trace_id: "trace-dynamic".into(),
        })
        .await
        .expect("dynamic root command executes");

    match execution {
        CommandExecution::Bridge { exchange, .. } => {
            assert_eq!(exchange.response.result.as_ref().unwrap()["mount"], "ops");
            assert!(exchange.response.result.as_ref().unwrap()["commands"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .any(|value| value.as_str() == Some("ping")));
        }
        other => panic!("expected bridge execution, got {other:?}"),
    }

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn bootstraps_dynamic_commands_from_installed_product_root() {
    let _env_guard = ENV_LOCK.lock().expect("env lock");
    let mut host = HostRuntime::new();
    let root = temp_dir("dynamic-commands-installed");
    let workspace = root.join("workspace");
    let product_root = root.join("install/lib/product");
    fs::create_dir_all(&workspace).expect("workspace created");
    fs::create_dir_all(product_root.join("dist/schema-roots/root-0"))
        .expect("schema root created");
    fs::write(
        product_root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "schema": { "entry": ["./dist/schema-roots/root-0/lania.schemas.js"] }
}
"#,
    )
    .expect("lan config written");
    fs::write(
        product_root.join("dist/schema-roots/root-0/package.json"),
        r#"{
  "type": "module"
}
"#,
    )
    .expect("schema package written");
    fs::write(
        product_root.join("dist/schema-roots/root-0/lania.schemas.js"),
        "export default { runtimeCommands: [{ mount: 'ops', commands: [{ name: 'ping', handler: async () => ({ result: { ok: true, exitCode: 0 } }) }] }] };\n",
    )
    .expect("runtime manifest written");

    host.initialize().await.expect("host initializes");
    std::env::set_var("LANIA_PRODUCT_ROOT", product_root.display().to_string());
    let summary = host
        .bootstrap_project_extensions_from_cwd_async(workspace.display().to_string())
        .await
        .expect("installed dynamic commands bootstrap");
    std::env::remove_var("LANIA_PRODUCT_ROOT");

    assert_eq!(summary.dynamic_commands, 1);
    assert!(host
        .command_specs()
        .iter()
        .any(|command| command.name == "ops"));

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn installed_product_root_commands_are_visible_to_cli_parser() {
    let _env_guard = ENV_LOCK.lock().expect("env lock");
    let mut host = HostRuntime::new();
    let root = temp_dir("dynamic-commands-installed-parser");
    let workspace = root.join("workspace");
    let product_root = root.join("install/lib/product");
    fs::create_dir_all(&workspace).expect("workspace created");
    fs::create_dir_all(product_root.join("dist/schema-roots/root-0"))
        .expect("schema root created");
    fs::write(
        product_root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "schema": { "entry": ["./dist/schema-roots/root-0/lania.schemas.js"] }
}
"#,
    )
    .expect("lan config written");
    fs::write(
        product_root.join("dist/schema-roots/root-0/package.json"),
        r#"{
  "type": "module"
}
"#,
    )
    .expect("schema package written");
    fs::write(
        product_root.join("dist/schema-roots/root-0/lania.schemas.js"),
        "export default { runtimeCommands: [{ mount: 'ops', commands: [{ name: 'ping', handler: async () => ({ result: { ok: true, exitCode: 0 } }) }] }] };\n",
    )
    .expect("runtime manifest written");

    host.initialize().await.expect("host initializes");
    std::env::set_var("LANIA_PRODUCT_ROOT", product_root.display().to_string());
    let _ = host
        .load_lan_config_snapshot_from_cwd_async(workspace.display().to_string())
        .await
        .ok();
    host.bootstrap_project_extensions_from_cwd_async(workspace.display().to_string())
        .await
        .expect("installed dynamic commands bootstrap");
    std::env::remove_var("LANIA_PRODUCT_ROOT");

    let mut commands = host.command_specs().to_vec();
    lania_command::apply_legacy_aliases(&mut commands);
    let matches = lania_command::build_cli("lan", "Lania CLI", "0.1.0", &commands, "en")
        .try_get_matches_from(["lan", "ops", "ping"])
        .expect("matches parse");
    let context = lania_command::command_context_from_matches(
        &commands,
        &matches,
        workspace.display().to_string(),
        "trace-installed",
    )
    .expect("dynamic command context");

    assert_eq!(context.handler_id, "dynamic.manifest.ops.ping.3");

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn invokes_project_lifecycle_hooks_during_command_execution() {
    let mut host = HostRuntime::new();
    host.register_plugin(Box::new(LifecyclePlugin))
        .expect("plugin registers");
    let root = temp_dir("lifecycle-hooks");
    fs::create_dir_all(root.join("scripts")).expect("scripts dir created");
    fs::write(
        root.join("lan.config.js"),
        r#"export default {
  plugins: ["./scripts/lania.plugin.js"],
  hooks: {
onCommandPreInit: [{ type: "plugin", kind: "parallel", plugin: "./scripts/lania.plugin.js", handler: "validateEnv" }],
onSuccess: [{ type: "plugin", kind: "parallel", plugin: "./scripts/lania.plugin.js", handler: "report" }]
  }
};
"#,
    )
    .expect("lan config written");
    fs::write(
        root.join("scripts/lania.plugin.js"),
        r#"export default {
  name: "demo-lifecycle",
  methods: ["hooks.invoke"],
  async handle(method, params) {
if (method !== "hooks.invoke") {
  return null;
}
return {
  result: { accepted: true, hook: params.hook, handler: params.handler },
  events: [
    {
      method: "event.log",
      params: { level: "info", message: `hook invoked: ${params.handler}` }
    }
  ]
};
  }
};
"#,
    )
    .expect("lifecycle plugin written");

    host.initialize().await.expect("host initializes");
    let summary = host
        .bootstrap_project_extensions_from_cwd_async(root.display().to_string())
        .await
        .expect("hooks bootstrap");

    assert_eq!(summary.lifecycle_hooks, 2);
    assert!(host
        .hook_snapshot()
        .registrations
        .iter()
        .any(|registration| registration.plugin == "./scripts/lania.plugin.js"));

    host.execute_command(&lania_command::CommandContext {
        cwd: root.display().to_string(),
        argv: lania_command::ParsedArgv::default(),
        handler_id: "lifecycle".into(),
        trace_id: "trace-lifecycle".into(),
    })
    .await
    .expect("command executes");

    let log_messages = host
        .logger()
        .entries()
        .into_iter()
        .map(|entry| entry.message)
        .collect::<Vec<_>>();
    assert!(log_messages
        .iter()
        .any(|message| message.contains("validateEnv")));
    assert!(log_messages
        .iter()
        .any(|message| message.contains("report")));

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn end_to_end_host_rpc_tools_work_through_real_node_bridge_process() {
    let _env_guard = ENV_LOCK.lock().expect("env lock");
    let mut host = HostRuntime::new();
    let root = temp_dir("host-rpc-e2e");
    fs::write(
        root.join("lan.config.js"),
        "export default { extensions: { dynamicCommands: true } };\n",
    )
    .expect("lan config written");
    fs::write(
        root.join("package.json"),
        r#"{ "name": "host-rpc-e2e", "scripts": { "build": "echo build" } }"#,
    )
    .expect("package.json written");
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 9.0\n").expect("lockfile written");
    fs::write(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'hosted',
          handler: async (ctx) => {
            const out = ctx.tools.path.resolve(ctx.cwd, '.host-rpc-output.json');
            await ctx.tools.fs.writeJson(out, { ok: true, from: 'schema' }, { space: 2 });
            const file = await ctx.tools.fs.readJson(out);
            const exec = await ctx.tools.exec.run({ program: '/bin/echo', args: ['hello-from-host'] });
            const pm = await ctx.tools.pm.detect();
            const runScript = await ctx.tools.pm.command.runScript('build');
            const git = await ctx.tools.git.status();
            const changedFiles = await ctx.tools.git.changedFiles();
            await ctx.tools.log.info('host rpc e2e log', { target: 'schema.e2e' });
            await ctx.tools.tasks.register('e2e-task', 'e2e task');
            await ctx.tools.tasks.start('e2e-task', 'e2e task');
            await ctx.tools.tasks.complete('e2e-task', 'done');
            await ctx.tools.progress.beginGroup('e2e-progress', 1, 'progress_bar');
            await ctx.tools.progress.advance('e2e-progress', 1);
            await ctx.tools.progress.finish('e2e-progress');
            const taskSnapshot = await ctx.tools.tasks.snapshot();
            const progressSummary = await ctx.tools.progress.summary();
            return ctx.tools.result.ok({
              file,
              execStdout: exec.stdout.trim(),
              pm,
              runScriptProgram: runScript.program,
              gitReady: git.ready,
              changedFiles,
              taskCount: taskSnapshot.tasks.length,
              progressItems: Array.isArray(progressSummary?.items) ? progressSummary.items.length : 0
            });
          }
        }
      ]
    }
  ]
};"#,
    )
    .expect("schema manifest written");
    let git_init = Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .expect("git init should run");
    assert!(
        git_init.status.success(),
        "git init should succeed: {:?}",
        git_init
    );

    host.initialize().await.expect("host initializes");
    // 这里是一个端到端测试：会启动真实的 node-bridge 子进程并扫描动态命令。
    // 在并行运行测试时，node-bridge 初始化与动态命令扫描可能出现极小概率的竞态，
    // 现象是 `dynamic_commands` 偶发为 0。
    //
    // 为了让测试更稳定（避免 CI 偶发波动），这里加入短暂重试：
    // - 每次重试都会重新触发一次 bootstrap
    // - 如果确实持续为 0，最终仍会 fail，便于发现真实回归
    let mut summary = None;
    for _ in 0..5 {
        let next = host
            .bootstrap_project_extensions_from_cwd_async(root.display().to_string())
            .await
            .expect("bootstrap succeeds");
        if next.dynamic_commands == 1 {
            summary = Some(next);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        summary = Some(next);
    }
    let summary = summary.expect("summary exists");
    assert_eq!(summary.dynamic_commands, 1);

    // `resolveDynamic` 的结果有时也会因为 node-bridge 的初始化/缓存刷新竞态而出现短暂缺失。
    // 这里同样加入短暂重试：直到找到 hosted handler（提取出 handlerId + target），或最终失败。
    let mut hosted_handler_id: Option<String> = None;
    let mut hosted_target: Option<serde_json::Value> = None;
    let mut last_handlers_len: Option<usize> = None;
    let mut last_resolve_result: Option<serde_json::Value> = None;
    // 这里适当拉长等待时间：在 CI 或并行测试环境下，node-bridge 的动态 import/缓存刷新
    // 偶发会慢于本地单测。我们用小步 backoff，避免固定 sleep 导致整体变慢。
    for attempt in 0..40 {
        let resolved = host
            .node_bridge()
            .call_async(host.node_bridge().request(
                "commands.resolveDynamic",
                json!({ "cwd": root.display().to_string() }),
            ))
            .await
            .expect("resolveDynamic succeeds");
        last_resolve_result = resolved.response.result.clone();
        let handlers = resolved
            .response
            .result
            .as_ref()
            .and_then(|value| value.get("handlers"))
            .and_then(|value| value.as_array())
            .expect("handlers array");
        last_handlers_len = Some(handlers.len());
        if let Some(handler) = handlers.iter().find(|item| {
            item.get("target")
                .and_then(|target| target.get("kind"))
                .and_then(|value| value.as_str())
                == Some("manifest_command")
                && item
                    .get("target")
                    .and_then(|target| target.get("path"))
                    .and_then(|value| value.as_array())
                    .is_some_and(|path| {
                        path.iter()
                            .any(|segment| segment.as_str() == Some("hosted"))
                    })
        }) {
            hosted_handler_id = Some(
                handler["handlerId"]
                    .as_str()
                    .expect("handlerId")
                    .to_string(),
            );
            hosted_target = Some(handler.get("target").cloned().expect("target"));
            break;
        }
        let backoff_ms = 25u64.saturating_mul((attempt + 1) as u64).min(250);
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
    }
    let handler_id = hosted_handler_id.unwrap_or_else(|| {
        panic!(
            "hosted handler exists (last handlers len: {:?}, last resolve result: {:?})",
            last_handlers_len, last_resolve_result
        )
    });
    let target = hosted_target.expect("hosted target exists");

    let invoked = host
        .node_bridge()
        .call_async(host.node_bridge().request(
            "command.invokeDynamic",
            json!({
                "cwd": root.display().to_string(),
                "handlerId": handler_id,
                "traceId": "trace-host-rpc-e2e",
                "argv": { "args": {}, "options": {} },
                "target": target,
            }),
        ))
        .await
        .expect("invokeDynamic succeeds");
    assert!(
        invoked.response.error.is_none(),
        "invokeDynamic returned error: {:?}",
        invoked.response.error
    );
    let raw = invoked.response.result.as_ref().expect("response result");
    eprintln!("invokeDynamic raw result: {raw}");
    // 实际上我们观察到这里可能存在两种返回形态（历史兼容）：
    // - { result: { ok, exitCode, data } }
    // - { ok, exitCode, data }
    let payload = raw
        .get("result")
        .and_then(|value| value.get("data"))
        .or_else(|| raw.get("data"))
        .expect("dynamic result payload");

    assert_eq!(payload["file"]["ok"], json!(true));
    assert_eq!(payload["execStdout"], json!("hello-from-host"));
    assert_eq!(payload["pm"], json!("pnpm"));
    assert_eq!(payload["runScriptProgram"], json!("pnpm"));
    assert_eq!(payload["gitReady"], json!(true));
    assert!(payload["changedFiles"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "package.json")));
    assert_eq!(payload["taskCount"], json!(1));
    assert_eq!(payload["progressItems"], json!(3));

    let log_messages = host
        .logger()
        .entries()
        .into_iter()
        .map(|entry| entry.message)
        .collect::<Vec<_>>();
    assert!(log_messages
        .iter()
        .any(|message| message.contains("host rpc e2e log")));
    assert_eq!(host.tasks().snapshot().len(), 1);
    assert_eq!(host.progress().summary().items.len(), 3);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn classifies_external_project_plugins() {
    let host = HostRuntime::new();
    let root = temp_dir("external-project-plugins");
    fs::create_dir_all(root.join("scripts")).expect("scripts dir exists");
    fs::write(
        root.join("lan.config.js"),
        "export default { plugins: ['@lania/plugin-custom-template', '@demo/project-plugin', './scripts/lania.plugin.ts', '/abs/not-allowed.js'] };\n",
    )
    .expect("lan config written");

    let summary = host
        .summary_for_cwd(root.display().to_string())
        .expect("summary builds");

    assert_eq!(summary.project_plugin_report.accepted_plugins.len(), 3);
    assert_eq!(
        summary.project_plugin_report.review_required_plugins.len(),
        1
    );
    assert_eq!(summary.project_plugin_report.rejected_plugins.len(), 1);
    assert_eq!(summary.project_node_plugins.len(), 3);

    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn summary_contains_plugin_manifest() {
    let mut host = HostRuntime::new();
    host.register_plugin(Box::new(LifecyclePlugin))
        .expect("plugin registers");
    host.initialize().await.expect("host initializes");

    let summary = host.summary();
    assert!(!summary.manifest.rust_plugins.is_empty());
    assert!(!summary.manifest.node_plugins.is_empty());
    assert!(!summary.manifest.supported_events.is_empty());
    assert_eq!(
        summary
            .manifest
            .project_config
            .as_ref()
            .map(|config| &config.build_tool),
        summary
            .project_config
            .as_ref()
            .map(|config| &config.build_tool)
    );
    assert_eq!(
        summary
            .manifest
            .project_plugin_report
            .accepted_plugins
            .len(),
        summary.project_plugin_report.accepted_plugins.len()
    );
}

#[test]
fn maps_extended_dynamic_prompt_kinds_from_wire() {
    let argv = lania_command::ParsedArgv::default();

    let password = super::dynamic::prompt_step_from_wire(
        &json!({
            "field": "token",
            "message": "Token",
            "kind": "password"
        }),
        &argv,
        "en",
    )
    .expect("password prompt step");
    assert!(matches!(
        password.kind,
        lania_prompt::PromptStepKind::Password
    ));

    let number = super::dynamic::prompt_step_from_wire(
        &json!({
            "field": "count",
            "message": "Count",
            "kind": "number"
        }),
        &argv,
        "en",
    )
    .expect("number prompt step");
    assert!(matches!(number.kind, lania_prompt::PromptStepKind::Number));

    let fuzzy = super::dynamic::prompt_step_from_wire(
        &json!({
            "field": "template",
            "message": "Template",
            "kind": "fuzzy_select"
        }),
        &argv,
        "en",
    )
    .expect("fuzzy prompt step");
    assert!(matches!(
        fuzzy.kind,
        lania_prompt::PromptStepKind::FuzzySelect
    ));
}

#[test]
fn redacts_secret_prompt_answer_maps() {
    let answers = std::collections::BTreeMap::from([
        ("password".to_string(), json!("super-secret")),
        ("user".to_string(), json!("demo")),
    ]);
    let redacted = super::dynamic::redact_secret_answer_map(&answers, &["password".to_string()]);

    assert_eq!(redacted["password"], json!("***"));
    assert_eq!(redacted["user"], json!("demo"));
}
