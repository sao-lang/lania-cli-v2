use serde_json::json;

use super::{
    ConfigMigrationPolicy, ConfigPluginRef, ConfigPluginSourceKind, ConfigPluginTrustLevel,
    ConfigService, ConfigValidationErrorCode, ReleaseProfile,
};

#[test]
fn parses_lan_config_snapshot() {
    let snapshot = ConfigService::load_lan_snapshot(&json!({
        "cwd": "/repo",
        "configPath": "/repo/lan.config.json",
        "exists": true,
        "supportedExtensions": [".js", ".json", ".ts"],
        "config": {
            "buildTool": "webpack",
            "lintTools": ["eslint", "prettier"],
            "plugins": [
                "@demo/project-plugin",
                {
                    "name": "demo-extra",
                    "package": "@demo/extra",
                    "methods": ["config.loadLan"]
                }
            ],
            "extensions": {
                "dynamicCommands": true
            },
            "schemaDiscovery": {
                "files": ["lania.schemas.ts"],
                "dirs": [".lania/schemas"],
                "allowExtensions": [".ts", ".js"]
            },
            "ui": {
                "locale": "zh",
                "output": {
                    "mode": "jsonl",
                    "events": "stream",
                    "pretty": true,
                    "includeHostState": false,
                    "includeBridgeExchange": false
                },
                "progress": {
                    "style": "bar",
                    "grouping": "operation"
                },
                "interaction": {
                    "mode": "interactive",
                    "timeoutMs": 30000,
                    "defaultStrategy": "fail"
                }
            },
            "commands": {
                "aliases": {
                    "gu": "api user get-user"
                },
                "shortcuts": {
                    "ping": "curl http://localhost:3000/ping"
                }
            },
            "custom": {
                "team": "demo"
            },
            "hooks": {
                "onCommandPreInit": [
                    { "type": "plugin", "kind": "parallel", "plugin": "./scripts/lania.plugin.ts", "handler": "validateEnv" }
                ]
            }
        },
        "buildTool": "webpack",
        "buildAdaptors": {
            "webpack": {"mode": "production"}
        },
        "lintAdaptors": {
            "eslint": {"cache": true}
        },
        "lintTools": ["eslint", "prettier"]
    }))
    .expect("snapshot parses");

    assert_eq!(snapshot.build_tool, "webpack");
    assert_eq!(snapshot.plugins.len(), 2);
    assert_eq!(snapshot.schema_version, 1);
    assert!(snapshot.validation_errors.is_empty());
    assert!(snapshot.extensions.dynamic_commands);
    assert_eq!(snapshot.ui.locale.as_deref(), Some("zh"));
    assert_eq!(snapshot.ui.output.mode, "jsonl");
    assert_eq!(snapshot.ui.progress.style, "bar");
    assert_eq!(snapshot.ui.interaction.default_strategy, "fail");
    assert_eq!(snapshot.schema_discovery.files, vec!["lania.schemas.ts"]);
    assert_eq!(
        snapshot.commands.aliases.get("gu").map(String::as_str),
        Some("api user get-user")
    );
    assert_eq!(
        snapshot.commands.shortcuts.get("ping").map(String::as_str),
        Some("curl http://localhost:3000/ping")
    );
    assert_eq!(snapshot.custom["team"], "demo");
    assert!(snapshot
        .hooks
        .get("onCommandPreInit")
        .is_some_and(|items| items[0].handler.as_deref() == Some("validateEnv")));
    assert_eq!(
        snapshot.plugins[1],
        ConfigPluginRef {
            name: "demo-extra".into(),
            package: "@demo/extra".into(),
            methods: vec!["config.loadLan".into()],
            declared_as: "@demo/extra".into(),
            source_kind: ConfigPluginSourceKind::Package,
            trust_level: ConfigPluginTrustLevel::ReviewRequired,
            loadable: true,
            reason: None,
        }
    );
}

#[test]
fn parses_release_config_snapshot() {
    let snapshot = ConfigService::load_lan_snapshot(&json!({
        "cwd": "/repo",
        "exists": true,
        "config": {
            "release": {
                "profile": "web-app",
                "env": "prod",
                "channel": "stable",
                "stateFile": ".lania/release.json",
                "verify": {
                    "lint": true,
                    "test": {"enabled": true, "command": "pnpm test:ci"}
                },
                "versioning": {
                    "enabled": true,
                    "source": "package.json",
                    "tagPrefix": "release-"
                },
                "changelog": "pnpm changelog",
                "artifact": {"enabled": true, "command": "pnpm build"},
                "deploy": {"provider": "custom", "command": "pnpm deploy:prod"},
                "postCheck": {"url": "https://example.com/healthz"},
                "git": {"commit": true, "tag": true, "push": true, "remote": "origin"}
            }
        }
    }))
    .expect("snapshot parses");

    let release = snapshot.release.expect("release config exists");
    assert_eq!(release.profile, ReleaseProfile::WebApp);
    assert_eq!(release.env.as_deref(), Some("prod"));
    assert_eq!(release.channel.as_deref(), Some("stable"));
    assert_eq!(release.state_file, ".lania/release.json");
    assert!(release.verify.lint.enabled);
    assert_eq!(release.verify.test.command.as_deref(), Some("pnpm test:ci"));
    assert_eq!(release.versioning.tag_prefix, "release-");
    assert_eq!(release.deploy.command.as_deref(), Some("pnpm deploy:prod"));
    assert_eq!(
        release.post_check.url.as_deref(),
        Some("https://example.com/healthz")
    );
    assert!(release.git.push);
}

#[test]
fn classifies_first_party_and_local_plugins() {
    let snapshot = ConfigService::load_lan_snapshot(&json!({
        "cwd": "/repo",
        "exists": true,
        "supportedExtensions": [".js", ".ts"],
        "config": {
            "plugins": [
                "@lania/plugin-custom-template",
                "./scripts/lania.plugin.ts",
                "/abs/not-allowed.js"
            ]
        }
    }))
    .expect("snapshot parses");

    assert_eq!(
        snapshot.plugins[0].trust_level,
        ConfigPluginTrustLevel::FirstParty
    );
    assert_eq!(
        snapshot.plugins[1].source_kind,
        ConfigPluginSourceKind::LocalPath
    );
    assert!(snapshot.plugins[1].loadable);
    assert!(snapshot.plugins[2].is_rejected());
}

#[test]
fn exposes_schema_doc_and_markdown() {
    let schema = ConfigService::lan_schema_doc();
    let markdown = ConfigService::lan_schema_markdown();

    assert_eq!(schema.version, 1);
    assert!(schema.fields.iter().any(|field| field.path == "$.plugins"));
    assert!(schema.fields.iter().any(|field| field.path == "$.ui"));
    assert!(schema.fields.iter().any(|field| field.path == "$.commands"));
    assert!(schema.fields.iter().any(|field| field.path == "$.custom"));
    assert!(schema
        .fields
        .iter()
        .any(|field| field.path == "$.pluginAllowlist"));
    assert!(markdown.contains("lan.config schema v1"));
    assert!(markdown.contains("`$.buildTool`"));
}

#[test]
fn reports_validation_errors_with_codes_and_paths() {
    let snapshot = ConfigService::load_lan_snapshot(&json!({
        "cwd": "/repo",
        "exists": true,
        "config": {
            "version": 2,
            "buildTool": 1,
            "lintTools": [true],
            "plugins": ["@demo/plugin", {"methods": []}],
            "release": {
                "profile": "desktop",
                "verify": {"lint": {"enabled": "yes"}},
                "postCheck": {"url": true}
            },
            "pluginTrustedSources": ["package", "invalid"],
            "pluginRequireSignature": "yes",
            "extensions": {"dynamicCommands": "yes", "unknownField": true},
            "ui": {"output": {"pretty": "yes", "unknownField": true}},
            "schemaDiscovery": {"files": [1], "unknownField": true},
            "commands": {"aliases": {"gu": true}, "unknownField": true},
            "hooks": {"onCommandPreInit": [{"plugin": true}]},
            "custom": true,
            "unknownField": true
        }
    }))
    .expect("snapshot parses");

    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.code == ConfigValidationErrorCode::UnsupportedVersion));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.buildTool"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.plugins[1]"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.pluginTrustedSources[1]"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.pluginRequireSignature"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.extensions.dynamicCommands"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.extensions.unknownField"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.ui.output.pretty"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.ui.output.unknownField"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.schemaDiscovery.files[0]"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.schemaDiscovery.unknownField"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.commands.aliases.gu"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.commands.unknownField"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.hooks.onCommandPreInit[0].plugin"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.hooks.onCommandPreInit[0].handler"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.custom"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.release.profile"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.release.verify.lint.enabled"));
    assert!(snapshot
        .validation_errors
        .iter()
        .any(|error| error.position.path == "$.release.postCheck.url"));
    assert_eq!(
        snapshot.version_strategy.policy,
        ConfigMigrationPolicy::Unsupported
    );
}

#[test]
fn parses_tool_config_snapshot() {
    let snapshot = ConfigService::load_tool_snapshot(&json!({
        "cwd": "/repo",
        "tool": "eslint",
        "configPath": "/repo/eslint.config.js",
        "exists": true,
        "resolved": true,
        "config": {
            "rules": {}
        }
    }))
    .expect("tool snapshot parses");

    assert_eq!(snapshot.tool, "eslint");
    assert!(snapshot.resolved);
    assert!(snapshot.validation_errors.is_empty());
}
