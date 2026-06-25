use lania_host::{
    capability::CapabilityContainer,
    plugin::{Plugin, PluginSetupContext},
    registry::{CommandHandlerRegistryImpl, CommandRegistry, CommandRegistryImpl, HandlerRegistry},
    HookBusImpl,
};
use lania_workflows::ReleaseMode;
use serde_json::json;

use super::{
    ReleaseCommandPlugin, HANDLER_ID, PLAN_HANDLER_ID, RESUME_HANDLER_ID, RUN_HANDLER_ID,
    STATUS_HANDLER_ID,
};

#[test]
fn registers_release_spec() {
    let plugin = ReleaseCommandPlugin;
    let mut commands = CommandRegistryImpl::new();
    let mut hooks = HookBusImpl::new();
    let mut capabilities = CapabilityContainer::new();
    let mut handlers = CommandHandlerRegistryImpl::new();

    plugin
        .setup(&mut PluginSetupContext {
            commands: &mut commands,
            hooks: &mut hooks,
            capabilities: &mut capabilities,
            handlers: &mut handlers,
        })
        .expect("plugin setup succeeds");

    let spec = &commands.commands()[0];
    assert_eq!(spec.name, "release");
    assert_eq!(spec.handler_id, HANDLER_ID);
    assert_eq!(spec.subcommands.len(), 4);
    assert!(handlers.get(HANDLER_ID).is_some());
    assert!(handlers.get(PLAN_HANDLER_ID).is_some());
    assert!(handlers.get(RUN_HANDLER_ID).is_some());
    assert!(handlers.get(RESUME_HANDLER_ID).is_some());
    assert!(handlers.get(STATUS_HANDLER_ID).is_some());
}

#[test]
fn builds_release_input_from_context() {
    let context = lania_command::CommandContext {
        cwd: "/repo".into(),
        argv: lania_command::ParsedArgv {
            args: Default::default(),
            options: [
                ("version".into(), json!("1.2.3")),
                ("tag".into(), json!("next")),
                ("profile".into(), json!("web-app")),
                ("env".into(), json!("prod")),
                ("channel".into(), json!("stable")),
                ("from".into(), json!("verify")),
                ("to".into(), json!("finalize")),
                ("skip".into(), json!("post_check,finalize")),
                ("state-file".into(), json!(".lania/custom-release.json")),
                ("apply".into(), json!(true)),
                ("dry-run".into(), json!(false)),
                ("yes".into(), json!(true)),
                ("publish".into(), json!(true)),
                ("changelog".into(), json!(true)),
                ("skip-git".into(), json!(false)),
            ]
            .into_iter()
            .collect(),
        },
        handler_id: HANDLER_ID.into(),
        trace_id: "trace-1".into(),
    };

    let input = ReleaseCommandPlugin::build_input(&context);
    assert_eq!(input.version.as_deref(), Some("1.2.3"));
    assert_eq!(input.tag.as_deref(), Some("next"));
    assert_eq!(input.profile.as_deref(), Some("web-app"));
    assert_eq!(input.env.as_deref(), Some("prod"));
    assert_eq!(input.channel.as_deref(), Some("stable"));
    assert_eq!(input.from_stage.as_deref(), Some("verify"));
    assert_eq!(input.to_stage.as_deref(), Some("finalize"));
    assert_eq!(input.skip_stages, vec!["post_check", "finalize"]);
    assert_eq!(
        input.state_file.as_deref(),
        Some(".lania/custom-release.json")
    );
    assert!(input.apply);
    assert!(input.yes);
    assert!(input.publish);
    assert!(input.changelog);
    assert!(!input.skip_git);
}

#[test]
fn maps_release_subcommand_modes() {
    for (handler_id, expected_mode) in [
        (HANDLER_ID, ReleaseMode::Plan),
        (PLAN_HANDLER_ID, ReleaseMode::Plan),
        (RUN_HANDLER_ID, ReleaseMode::Run),
        (RESUME_HANDLER_ID, ReleaseMode::Resume),
        (STATUS_HANDLER_ID, ReleaseMode::Status),
    ] {
        let context = lania_command::CommandContext {
            cwd: "/repo".into(),
            argv: lania_command::ParsedArgv::default(),
            handler_id: handler_id.to_string(),
            trace_id: "trace".into(),
        };
        let input = ReleaseCommandPlugin::build_input(&context);
        assert_eq!(input.mode, expected_mode);
    }
}
