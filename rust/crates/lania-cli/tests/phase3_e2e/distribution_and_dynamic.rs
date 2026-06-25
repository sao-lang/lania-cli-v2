//! 分发形态与动态命令输出链路 e2e。
use super::common::*;
use super::*;
use lania_host::{plugin::EmptyPlugin, Host, HostRuntime};
use lania_plugins_command_add::AddCommandPlugin;
use lania_plugins_command_build::BuildCommandPlugin;
use lania_plugins_command_create::CreateCommandPlugin;
use lania_plugins_command_dev::DevCommandPlugin;
use lania_plugins_command_generate::GenerateCommandPlugin;
use lania_plugins_command_lint::LintCommandPlugin;
use lania_plugins_command_locale::ConfigCommandPlugin;
use lania_plugins_command_release::ReleaseCommandPlugin;
use lania_plugins_command_sync::SyncCommandPlugin;
use lania_plugins_command_template::TemplateCommandPlugin;
use lania_plugins_command_tools::ToolsCommandPlugin;

fn register_cli_plugins(host: &mut HostRuntime) {
    host.register_plugin(Box::new(EmptyPlugin))
        .expect("empty plugin registers");
    host.register_plugin(Box::new(DevCommandPlugin))
        .expect("dev plugin registers");
    host.register_plugin(Box::new(BuildCommandPlugin))
        .expect("build plugin registers");
    host.register_plugin(Box::new(LintCommandPlugin))
        .expect("lint plugin registers");
    host.register_plugin(Box::new(CreateCommandPlugin))
        .expect("create plugin registers");
    host.register_plugin(Box::new(AddCommandPlugin))
        .expect("add plugin registers");
    host.register_plugin(Box::new(GenerateCommandPlugin))
        .expect("generate plugin registers");
    host.register_plugin(Box::new(ReleaseCommandPlugin))
        .expect("release plugin registers");
    host.register_plugin(Box::new(SyncCommandPlugin))
        .expect("sync plugin registers");
    host.register_plugin(Box::new(TemplateCommandPlugin))
        .expect("template plugin registers");
    host.register_plugin(Box::new(ToolsCommandPlugin))
        .expect("tools plugin registers");
    host.register_plugin(Box::new(ConfigCommandPlugin))
        .expect("config plugin registers");
}

#[test]
fn lan_npm_install_layout_smoke() {
    let root = temp_dir("npm-install-layout");
    let bridge_dir = stage_installed_bridge_package(&root);
    let project = root.join("project");
    write_file(
        project.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );

    let output = run_cli(
        &project,
        &["build", "--mode", "production"],
        &[(
            "LANIA_NODE_BRIDGE_DIR",
            bridge_dir.to_str().expect("bridge dir path"),
        )],
    );
    let json = parse_stdout_json(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(json["exchange"]["response"]["result"]["tool"], "vite");
    assert!(json["exchange"]["response"]["result"]["watch"].is_boolean());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_binary_distribution_smoke() {
    let root = temp_dir("binary-distribution");
    let install_root = root.join("install");
    let bin_dir = install_root.join("bin");
    fs::create_dir_all(&bin_dir).expect("bin dir created");
    let staged_binary = bin_dir.join("lania-cli");
    fs::copy(env!("CARGO_BIN_EXE_lania-cli"), &staged_binary).expect("binary copied");
    let _bridge_dir = stage_installed_bridge_package(&install_root);

    let project = root.join("project");
    write_file(
        project.join("lan.config.js"),
        "export default { buildTool: 'vite' };\n",
    );
    let output = run_program(&staged_binary, &project, &["build"], &[]);
    let json = parse_stdout_json(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["exchange"]["response"]["result"]["tool"], "vite");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_packed_product_wrapper_runs_dynamic_command_in_installed_mode() {
    let root = temp_dir("packed-product-wrapper");
    let product = root.join("product");
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace created");
    write_file(
        product.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@demo/acme",
    "binaryName": "acme"
  },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(
        product.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'inspect-installed',
          about: 'Inspect installed runtime context',
          handler: async (ctx) => ({
            result: {
              mode: ctx.runtime.mode,
              workspaceRoot: ctx.runtime.workspaceRoot,
              productRoot: ctx.runtime.productRoot,
              invocationCwd: ctx.runtime.invocationCwd,
              schemaRoot: ctx.product.schemaRoot,
              exitCode: 0
            }
          })
        }
      ]
    }
  ]
};
"#,
    );

    let build_output = run_cli(&product, &["product", "build"], &[]);
    assert_eq!(
        build_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    let pack_output = run_cli(&product, &["product", "pack"], &[]);
    assert_eq!(
        pack_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&pack_output.stdout),
        String::from_utf8_lossy(&pack_output.stderr)
    );

    let wrapper = product.join(".lania/pack/product/install-root/bin/acme");
    let binary = env!("CARGO_BIN_EXE_lania-cli");
    let wrapper_output = run_program(
        &wrapper,
        &workspace,
        &["ops", "inspect-installed"],
        &[("LANIA_PRODUCT_HOST_BINARY", binary)],
    );

    assert_eq!(
        wrapper_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&wrapper_output.stdout),
        String::from_utf8_lossy(&wrapper_output.stderr)
    );
    let json = parse_stdout_json(&wrapper_output);
    let payload = &json["exchange"]["response"]["result"]["result"]["result"];
    assert_eq!(payload["mode"], "installed");
    let expected_workspace = fs::canonicalize(&workspace)
        .expect("workspace canonicalized")
        .display()
        .to_string();
    let expected_product_root =
        fs::canonicalize(product.join(".lania/pack/product/install-root/lib/product"))
            .expect("product root canonicalized")
            .display()
            .to_string();
    let expected_schema_root = fs::canonicalize(
        product.join(".lania/pack/product/install-root/lib/product/dist/schema-roots/root-0"),
    )
    .expect("schema root canonicalized")
    .display()
    .to_string();
    assert_eq!(payload["workspaceRoot"], expected_workspace);
    assert_eq!(payload["invocationCwd"], expected_workspace);
    assert_eq!(payload["productRoot"], expected_product_root);
    assert_eq!(payload["schemaRoot"], expected_schema_root);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_publish_product_creates_publish_ready_artifact() {
    let root = temp_dir("publish-product-artifact");
    let product = root.join("product");
    fs::create_dir_all(product.join("templates/base")).expect("templates dir created");
    write_file(
        product.join("package.json"),
        r#"{
  "name": "@demo/acme",
  "version": "1.2.3"
}
"#,
    );
    write_file(
        product.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@demo/acme",
    "binaryName": "acme",
    "templatesDir": "./templates"
  },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(
        product.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'hello',
          about: 'hello',
          handler: async () => ({ result: { ok: true, exitCode: 0 } })
        }
      ]
    }
  ]
};
"#,
    );
    write_file(
        product.join("templates/base/template.json"),
        r#"{
  "id": "base"
}
"#,
    );
    let platform_binaries_dir = product.join("fixtures/platform-binaries");
    let linux_binary = platform_binaries_dir.join("linux-x64/lania-cli");
    write_file(&linux_binary, "#!/bin/sh\nexit 0\n");

    let build_output = run_cli(&product, &["product", "build"], &[]);
    assert_eq!(build_output.status.code(), Some(0));
    let pack_output = run_cli(&product, &["product", "pack"], &[]);
    assert_eq!(pack_output.status.code(), Some(0));
    let publish_output = run_cli(
        &product,
        &[
            "product",
            "publish",
            "--dist-tag",
            "next",
            "--channel",
            "beta",
            "--platform-binaries-dir",
            &platform_binaries_dir.display().to_string(),
        ],
        &[],
    );
    assert_eq!(
        publish_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&publish_output.stdout),
        String::from_utf8_lossy(&publish_output.stderr)
    );

    let publish_root = product.join(".lania/publish/product/npm-package");
    let package_json = fs::read_to_string(publish_root.join("package.json"))
        .expect("published package json readable");
    assert!(package_json.contains(r#""name": "@demo/acme""#));
    assert!(package_json.contains(r#""version": "1.2.3""#));
    assert!(package_json.contains(r#""acme": "./bin/acme.mjs""#));
    assert!(package_json.contains(r#""@lania-cli/cli":"#));

    let wrapper =
        fs::read_to_string(publish_root.join("bin/acme.mjs")).expect("published wrapper readable");
    assert!(wrapper.contains("LANIA_PRODUCT_ROOT"));
    assert!(wrapper.contains("LANIA_NODE_BRIDGE_DIR"));
    assert!(wrapper.contains("@lania-cli/cli"));

    let report = fs::read_to_string(publish_root.join("publish-report.json"))
        .expect("publish report readable");
    assert!(report.contains(r#""kind": "product_publish""#));
    assert!(report.contains(r#""mode": "npm_package""#));
    let manifest = fs::read_to_string(publish_root.join("publish-manifest.json"))
        .expect("publish manifest readable");
    assert!(manifest.contains(r#""kind": "product_publish_manifest""#));
    assert!(manifest.contains(r#""mode": "registry_plan""#));
    assert!(manifest.contains(r#""distTag": "next""#));
    assert!(manifest.contains(r#""channel": "beta""#));
    assert!(manifest.contains(r#""role": "product""#));
    assert!(manifest.contains(r#""role": "official_cli""#));
    let publish_report = read_json_file(publish_root.join("publish-report.json"));
    assert_eq!(
        publish_report["experimental"]["registryPublish"]["distTag"],
        "next"
    );
    assert_eq!(
        publish_report["experimental"]["registryPublish"]["channel"],
        "beta"
    );
    assert_eq!(
        publish_report["bundle"]["platformMatrix"][0]["packageName"],
        "@lania-cli/cli-darwin-arm64"
    );
    let matrix_status = publish_report["bundle"]["platformMatrix"][0]["status"]
        .as_str()
        .expect("platform matrix status present");
    assert!(matches!(matrix_status, "binary_missing" | "package_missing" | "ready"));
    let publish_manifest = read_json_file(publish_root.join("publish-manifest.json"));
    assert_eq!(
        publish_manifest["platformMatrix"][0]["status"],
        publish_report["bundle"]["platformMatrix"][0]["status"]
    );
    let linux_entry = publish_report["bundle"]["platformMatrix"]
        .as_array()
        .expect("platform matrix array")
        .iter()
        .find(|entry| entry["packageName"] == "@lania-cli/cli-linux-x64")
        .expect("linux-x64 matrix entry present");
    assert_eq!(linux_entry["status"], "ready");
    assert_eq!(linux_entry["source"], "request");
    let entries = fs::read_dir(&publish_root).expect("publish dir readable");
    assert!(entries.filter_map(|entry| entry.ok()).any(|entry| entry
        .path()
        .extension()
        .map(|ext| ext == "tgz")
        .unwrap_or(false)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_inspect_product_human_mode_e2e() {
    let root = temp_dir("inspect-product-human");
    let product = root.join("product");
    fs::create_dir_all(product.join("product/templates/base")).expect("product dirs created");
    write_file(
        product.join("package.json"),
        r#"{
  "name": "@demo/acme",
  "version": "1.2.3"
}
"#,
    );
    write_file(
        product.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": {
      "mode": "human"
    }
  },
  "product": {
    "name": "@demo/acme",
    "binaryName": "acme",
    "templatesDir": "./product/templates"
  },
  "schema": {
    "entry": "./product/lania.schemas.ts"
  }
}
"#,
    );
    write_file(
        product.join("product/lania.schemas.ts"),
        r#"export default {
  commands: [{ name: 'hello', workflow: 'hello' }],
  workflows: {
    hello: async () => undefined
  }
};
"#,
    );
    write_file(
        product.join("product/templates/base/template.json"),
        r#"{
  "id": "base"
}
"#,
    );

    let build_output = run_cli(&product, &["product", "build"], &[]);
    assert_eq!(build_output.status.code(), Some(0));

    let inspect_output = run_cli(&product, &["product", "inspect"], &[]);
    let stdout = String::from_utf8_lossy(&inspect_output.stdout);

    assert_eq!(inspect_output.status.code(), Some(0));
    assert!(stdout.contains("Product Development Diagnostics"));
    // inspect --compat should render a compat section in human mode.
    let inspect_compat_output = run_cli(&product, &["product", "inspect", "--compat"], &[]);
    let inspect_compat_stdout = String::from_utf8_lossy(&inspect_compat_output.stdout);
    assert_eq!(inspect_compat_output.status.code(), Some(0));
    assert!(inspect_compat_stdout.contains("Compatibility"));
    assert!(stdout.contains("Overview"));
    assert!(stdout.contains("- Product Package: @demo/acme"));
    assert!(stdout.contains("- Binary: acme"));
    assert!(stdout.contains("Schema"));
    assert!(stdout.contains("- Schema Entries:"));
    assert!(stdout.contains("  - ./product/lania.schemas.ts"));
    assert!(stdout.contains("Artifacts"));
    assert!(stdout.contains("- build: ready (.lania/build/product)"));
    assert!(stdout.contains("Checks"));
    assert!(stdout.contains("Next Steps"));
    assert!(stdout.contains(
        "Run `lan product pack` to prepare the install-root artifact for product validation."
    ));
    assert!(
        !stdout.contains("Bridge Command Completed"),
        "inspect product in human mode should render specialized diagnostics: {stdout}"
    );

    // doctor product should also render a human-friendly diagnostics block.
    let doctor_output = run_cli(&product, &["product", "doctor"], &[]);
    let doctor_stdout = String::from_utf8_lossy(&doctor_output.stdout);
    assert_eq!(doctor_output.status.code(), Some(0));
    assert!(doctor_stdout.contains("Product Doctor Diagnostics"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn phase2_product_authoring_full_chain_demo_e2e() {
    let root = temp_dir("phase2-product-full-chain");
    ensure_node_bridge_dist();

    // 1) Generate a demo product scaffold.
    let gen_output = run_cli(
        &root,
        &[
            "product",
            "generate",
            "--preset",
            "demo",
            "--name",
            "Acme CLI",
            "--binary-name",
            "acme",
            "--output-dir",
            "product",
            "--force",
        ],
        &[],
    );
    assert_eq!(gen_output.status.code(), Some(0));

    let product = root.join("product");

    // 2) Inspect product (human mode in demo preset).
    let inspect_output = run_cli(&product, &["product", "inspect"], &[]);
    let inspect_stdout = String::from_utf8_lossy(&inspect_output.stdout);
    let inspect_stderr = String::from_utf8_lossy(&inspect_output.stderr);
    assert_eq!(
        inspect_output.status.code(),
        Some(0),
        "inspect failed.\nstdout:\n{}\nstderr:\n{}",
        inspect_stdout,
        inspect_stderr
    );
    assert!(inspect_stdout.contains("Product Development Diagnostics"));
    assert!(inspect_stdout.contains("Next Steps"));

    // 3) Ensure product templates are discoverable via `lan template` (product.templatesDir).
    let template_list = run_cli(&product, &["template"], &[]);
    let template_stdout = String::from_utf8_lossy(&template_list.stdout);
    assert_eq!(template_list.status.code(), Some(0));
    assert!(
        template_stdout.contains("demo-app"),
        "template list should include product template: {template_stdout}"
    );

    // 4) Run a product command in development mode through `product dev`.
    let dev_output = run_cli(&product, &["product", "dev", "hello", "--path", "."], &[]);
    assert_eq!(dev_output.status.code(), Some(0));

    // 4.1) Grouped product commands should work as aliases for the scattered command surface.
    assert_eq!(
        run_cli(&product, &["product", "inspect"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "doctor"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "build"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "pack"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "publish"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "dev", "hello", "--path", "."], &[]).status.code(),
        Some(0)
    );

    // 5) Ensure the product-provided template workflow writes the expected report.
    let run_output = run_cli(&product, &["product-template"], &[]);
    assert_eq!(
        run_output.status.code(),
        Some(0),
        "product-template failed.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run_output.stdout),
        String::from_utf8_lossy(&run_output.stderr)
    );
    let product_template_report = product.join(".lania/reports/product-templates.json");
    assert!(
        product_template_report.exists(),
        "expected product template workflow output: {}",
        product_template_report.display()
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_publish_product_can_execute_manifest_dry_run() {
    let root = temp_dir("publish-product-execute-dry-run");
    ensure_node_bridge_dist();

    let product = root.join("product");
    fs::create_dir_all(product.join("product/templates/base")).expect("product dirs created");
    write_file(
        product.join("package.json"),
        r#"{
  "name": "@demo/acme",
  "version": "1.2.3"
}
"#,
    );
    write_file(
        product.join("lan.config.cjs"),
        r#"module.exports = {
  extensions: { dynamicCommands: true },
  product: {
    name: '@demo/acme',
    binaryName: 'acme',
    templatesDir: './product/templates'
  },
  schema: {
    entry: './product/lania.schemas.ts'
  }
};
"#,
    );
    write_file(
        product.join("product/lania.schemas.ts"),
        r#"export default {
  commands: [{ name: 'hello', workflow: 'hello' }],
  workflows: {
    hello: async () => undefined
  }
};
"#,
    );
    write_file(
        product.join("product/templates/base/template.json"),
        r#"{
  "id": "base"
}
"#,
    );
    let linux_binary = product.join("fixtures/lania-cli-linux-x64");
    write_file(&linux_binary, "#!/bin/sh\nexit 0\n");

    assert_eq!(run_cli(&product, &["product", "build"], &[]).status.code(), Some(0));
    assert_eq!(run_cli(&product, &["product", "pack"], &[]).status.code(), Some(0));

    let fake_bin_dir = root.join("fake-bin");
    fs::create_dir_all(&fake_bin_dir).expect("fake bin dir created");
    let fake_npm = fake_bin_dir.join("npm");
    let fake_log = root.join("fake-npm.log");
    write_file(
        &fake_npm,
        "#!/bin/sh\nif [ \"$1\" = \"whoami\" ]; then\n  echo fake-user\n  exit 0\nfi\nif [ \"$1\" = \"view\" ]; then\n  exit 1\nfi\npython3 -c \"import json, os, sys; open(os.environ['LANIA_FAKE_NPM_LOG'], 'a').write(json.dumps({'cwd': os.getcwd(), 'args': sys.argv[1:]}) + '\\n')\" \"$@\"\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&fake_npm)
            .expect("fake npm metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_npm, permissions).expect("fake npm permissions set");
    }

    let platform_binary_paths =
        format!(r#"{{"linux-x64":"{}"}}"#, linux_binary.display());
    let execute_output = run_cli(
        &product,
        &[
            "product",
            "publish",
            "--execute",
            "--dry-run",
            "--platform-binary-paths",
            &platform_binary_paths,
            "--npm-bin",
            fake_npm.to_str().expect("fake npm path"),
        ],
        &[
            (
                "LANIA_FAKE_NPM_LOG",
                fake_log.to_str().expect("fake log path"),
            ),
        ],
    );
    assert_eq!(
        execute_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&execute_output.stdout),
        String::from_utf8_lossy(&execute_output.stderr)
    );

    let publish_root = product.join(".lania/publish/product/npm-package");
    let manifest = read_json_file(publish_root.join("publish-manifest.json"));
    assert_eq!(manifest["execution"]["executed"], true);
    assert_eq!(manifest["execution"]["dryRun"], true);
    assert_eq!(manifest["execution"]["preflight"]["checked"], true);
    assert_eq!(manifest["execution"]["preflight"]["actor"], "fake-user");
    assert_eq!(manifest["execution"]["failedStepId"], serde_json::Value::Null);
    assert_eq!(manifest["execution"]["lastError"], serde_json::Value::Null);
    let report = read_json_file(publish_root.join("publish-report.json"));
    assert_eq!(
        report["experimental"]["registryPublish"]["execution"],
        manifest["execution"]
    );

    let log_lines = fs::read_to_string(&fake_log).expect("fake npm log readable");
    let log_entries: Vec<serde_json::Value> = log_lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fake npm log entry"))
        .collect();
    assert!(!log_entries.is_empty(), "publish executor should invoke fake npm");
    let publish_entries: Vec<&serde_json::Value> = log_entries
        .iter()
        .filter(|entry| entry["args"].as_array().and_then(|args| args.first()).map(|arg| arg == "publish").unwrap_or(false))
        .collect();
    assert!(!publish_entries.is_empty(), "publish executor should invoke fake npm publish");
    assert!(publish_entries
        .iter()
        .all(|entry| entry["args"].as_array().map(|args| args.iter().any(|arg| arg == "--dry-run")).unwrap_or(false)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_publish_product_tarball_installs_and_runs() {
    let root = temp_dir("publish-product-tarball-install");
    let product = root.join("product");
    fs::create_dir_all(product.join("templates/base")).expect("templates dir created");
    write_file(
        product.join("package.json"),
        r#"{
  "name": "@demo/acme",
  "version": "1.2.3"
}
"#,
    );
    write_file(
        product.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@demo/acme",
    "binaryName": "acme",
    "templatesDir": "./templates"
  },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(
        product.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'hello',
          about: 'hello',
          handler: async (ctx) => ({
            result: {
              mode: ctx.runtime.mode,
              workspaceRoot: ctx.runtime.workspaceRoot,
              productRoot: ctx.runtime.productRoot,
              schemaRoot: ctx.runtime.schemaRoot,
              templatesDir: ctx.product.templatesDir,
              exitCode: 0
            }
          })
        }
      ]
    }
  ]
};
"#,
    );
    write_file(
        product.join("templates/base/template.json"),
        r#"{
  "id": "base"
}
"#,
    );

    assert_eq!(
        run_cli(&product, &["product", "build"], &[]).status.code(),
        Some(0)
    );
    assert_eq!(
        run_cli(&product, &["product", "pack"], &[]).status.code(),
        Some(0)
    );
    let staged_platform_binary = repo_root().join("npm/cli-darwin-arm64/bin/lania-cli");
    let staged_platform_backup = if staged_platform_binary.exists() {
        Some(fs::read(&staged_platform_binary).expect("existing staged binary readable"))
    } else {
        None
    };
    if let Some(parent) = staged_platform_binary.parent() {
        fs::create_dir_all(parent).expect("official staging bin dir created");
    }
    fs::copy(env!("CARGO_BIN_EXE_lania-cli"), &staged_platform_binary)
        .expect("official staging binary copied");
    let publish_output = run_cli(&product, &["product", "publish"], &[]);
    if publish_output.status.code() != Some(0) {
        match staged_platform_backup.as_ref() {
            Some(bytes) => {
                fs::write(&staged_platform_binary, bytes).expect("staged binary restored");
            }
            None => {
                remove_path_if_exists(&staged_platform_binary);
            }
        }
    }
    assert_eq!(
        publish_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&publish_output.stdout),
        String::from_utf8_lossy(&publish_output.stderr)
    );

    let publish_root = product.join(".lania/publish/product/npm-package");
    let publish_report = read_json_file(publish_root.join("publish-report.json"));
    let tarball = publish_root.join(
        publish_report["tarball"]
            .as_str()
            .expect("publish tarball path present")
            .trim_start_matches("./"),
    );
    let cli_tarball = publish_root.join(
        publish_report["bundle"]["cliTarball"]
            .as_str()
            .expect("cli tarball path present")
            .trim_start_matches("./"),
    );
    let platform_tarball = publish_root.join(
        publish_report["bundle"]["platformTarball"]
            .as_str()
            .expect("platform tarball path present")
            .trim_start_matches("./"),
    );
    assert!(cli_tarball.exists(), "official cli tarball should exist");
    assert!(
        platform_tarball.exists(),
        "platform binary tarball should exist"
    );
    assert_eq!(
        publish_report["bundle"]["platformTarballs"][0]["source"],
        "official_staging"
    );
    assert_eq!(
        publish_report["bundle"]["platformMatrix"][0]["status"],
        "ready"
    );
    assert_eq!(
        publish_report["bundle"]["platformMatrix"][0]["source"],
        "official_staging"
    );
    assert_eq!(
        publish_report["bundle"]["platformMatrix"][0]["tarball"],
        publish_report["bundle"]["platformTarballs"][0]["tarball"]
    );
    let publish_manifest = read_json_file(publish_root.join("publish-manifest.json"));
    assert_eq!(publish_manifest["platformMatrix"][0]["status"], "ready");

    let consumer = root.join("consumer");
    fs::create_dir_all(&consumer).expect("consumer dir created");
    write_file(
        consumer.join("package.json"),
        r#"{
  "name": "consumer",
  "version": "1.0.0",
  "private": true
}
"#,
    );

    let home = isolated_home(&root);
    let install_output = run_program(
        std::path::Path::new("npm"),
        &consumer,
        &[
            "install",
            "--ignore-scripts",
            tarball.to_str().expect("tarball path"),
            cli_tarball.to_str().expect("cli tarball path"),
            platform_tarball.to_str().expect("platform tarball path"),
        ],
        &[("HOME", home.as_str())],
    );
    assert_eq!(
        install_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );

    let installed_binary = consumer.join("node_modules/.bin/acme");
    let help_output = run_program(
        &installed_binary,
        &consumer,
        &["--help"],
        &[("HOME", home.as_str())],
    );
    assert_eq!(
        help_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&help_output.stdout),
        String::from_utf8_lossy(&help_output.stderr)
    );
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(help_stdout.contains("ops"));

    let ops_help_output = run_program(
        &installed_binary,
        &consumer,
        &["ops", "--help"],
        &[("HOME", home.as_str())],
    );
    assert_eq!(
        ops_help_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ops_help_output.stdout),
        String::from_utf8_lossy(&ops_help_output.stderr)
    );
    let ops_help_stdout = String::from_utf8_lossy(&ops_help_output.stdout);
    assert!(ops_help_stdout.contains("hello"));

    let run_output = run_program(
        &installed_binary,
        &consumer,
        &["ops", "hello"],
        &[("HOME", home.as_str())],
    );
    assert_eq!(
        run_output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run_output.stdout),
        String::from_utf8_lossy(&run_output.stderr)
    );

    let json = parse_stdout_json(&run_output);
    let payload = &json["exchange"]["response"]["result"]["result"]["result"];
    assert_eq!(payload["mode"], "installed");
    assert_eq!(
        payload["workspaceRoot"],
        fs::canonicalize(&consumer)
            .expect("consumer canonicalized")
            .display()
            .to_string()
    );
    assert!(payload["productRoot"]
        .as_str()
        .map(|value| value.contains("node_modules/@demo/acme/lib/product"))
        .unwrap_or(false));
    assert!(payload["schemaRoot"]
        .as_str()
        .map(|value| value.contains("node_modules/@demo/acme/lib/product/dist/schema-roots/root-0"))
        .unwrap_or(false));
    assert!(payload["templatesDir"]
        .as_str()
        .map(|value| value.contains("node_modules/@demo/acme/lib/product/templates"))
        .unwrap_or(false));

    match staged_platform_backup {
        Some(bytes) => {
            fs::write(&staged_platform_binary, bytes).expect("staged binary restored");
        }
        None => {
            remove_path_if_exists(&staged_platform_binary);
        }
    }

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn lan_cli_main_flow_bootstraps_installed_product_commands() {
    let root = temp_dir("packed-product-main-flow");
    let product = root.join("product");
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace created");
    write_file(
        product.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "product": {
    "name": "@demo/acme",
    "binaryName": "acme"
  },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(
        product.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'inspect-installed',
          about: 'Inspect installed runtime context',
          handler: async () => ({ result: { ok: true, exitCode: 0 } })
        }
      ]
    }
  ]
};
"#,
    );
    let build_output = run_cli(&product, &["product", "build"], &[]);
    assert_eq!(build_output.status.code(), Some(0));
    let pack_output = run_cli(&product, &["product", "pack"], &[]);
    assert_eq!(pack_output.status.code(), Some(0));

    let install_root = product.join(".lania/pack/product/install-root");
    let product_root = install_root.join("lib/product");
    let bridge_dir = install_root.join("lib/node-bridge");

    let mut host = HostRuntime::new();
    register_cli_plugins(&mut host);
    host.initialize().await.expect("host initializes");
    std::env::set_var("LANIA_PRODUCT_ROOT", product_root.display().to_string());
    std::env::set_var("LANIA_NODE_BRIDGE_DIR", bridge_dir.display().to_string());
    let installed_snapshot = host
        .load_lan_config_snapshot_from_cwd_async(product_root.display().to_string())
        .await
        .expect("installed snapshot loads");
    let summary = host
        .bootstrap_project_extensions_from_cwd_async(workspace.display().to_string())
        .await
        .expect("installed product bootstrap");
    let mut commands = host.command_specs().to_vec();
    lania_command::apply_legacy_aliases(&mut commands);
    let command_names = commands
        .iter()
        .map(|command| command.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        summary.dynamic_commands, 1,
        "snapshot_extensions={}, snapshot_path={:?}, summary={summary:?}, commands={command_names:?}",
        installed_snapshot.extensions.dynamic_commands,
        installed_snapshot.config_path
    );
    assert!(
        commands.iter().any(|command| command.name == "ops"),
        "snapshot_extensions={}, snapshot_path={:?}, summary={summary:?}, commands={command_names:?}",
        installed_snapshot.extensions.dynamic_commands,
        installed_snapshot.config_path
    );
    let matches = lania_command::build_cli("lan", "Lania CLI", "0.1.0", &commands, "en")
        .try_get_matches_from(["lan", "ops", "inspect-installed"])
        .expect("dynamic command parses");
    let context = lania_command::command_context_from_matches(
        &commands,
        &matches,
        workspace.display().to_string(),
        "trace-installed",
    )
    .expect("command context resolves");
    std::env::remove_var("LANIA_PRODUCT_ROOT");
    std::env::remove_var("LANIA_NODE_BRIDGE_DIR");
    assert_eq!(
        context.handler_id,
        "dynamic.manifest.ops.inspect-installed.3"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dynamic_command_prompt_and_jsonl_output_e2e() {
    let root = temp_dir("dynamic-command-jsonl");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": {
      "mode": "jsonl",
      "events": "stream",
      "includeHostState": false,
      "includeBridgeExchange": false
    },
    "interaction": {
      "mode": "auto",
      "defaultStrategy": "use_defaults"
    },
    "progress": {
      "style": "none"
    }
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'ping',
          about: 'Ping a runtime handler',
          options: [{ long: 'endpoint', valueKind: 'string', help: 'Endpoint', required: true }],
          prompt: [{ field: 'endpoint', message: 'Endpoint?', kind: 'input', whenMissing: ['endpoint'] }],
          hooks: {
            preRun: [async (ctx) => ({ events: [{ method: 'event.log', params: { level: 'info', message: 'preRun' } }] })]
          },
          handler: async (ctx) => {
            const endpoint = ctx?.argv?.options?.endpoint ?? null;
            return {
              result: { ok: true, endpoint, exitCode: 0 },
              events: [{ method: 'event.log', params: { level: 'info', message: 'ping invoked' } }]
            };
          }
        }
      ]
    }
  ]
};
"#,
    );

    let prompt_answers = serde_json::json!({
        "endpoint": "http://127.0.0.1:1"
    })
    .to_string();
    let output = run_cli(
        &root,
        &["ops", "ping"],
        &[("LANIA_PROMPT_ANSWERS_JSON", prompt_answers.as_str())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    assert_eq!(output.status.code(), Some(0));
    assert!(
        lines.len() >= 2,
        "expected event + result lines, got: {stdout}"
    );

    let event_line: serde_json::Value =
        serde_json::from_str(lines[0]).expect("first jsonl event line");
    let result_line: serde_json::Value =
        serde_json::from_str(lines.last().expect("result line")).expect("final jsonl result line");

    assert_eq!(event_line["kind"], "event");
    assert_eq!(result_line["kind"], "result");
    assert_eq!(result_line["payload"]["kind"], "bridge");
    assert_eq!(
        result_line["payload"]["result"]["result"]["endpoint"],
        "http://127.0.0.1:1"
    );
    assert!(result_line["payload"]["host_state"].is_null());
    assert!(result_line["payload"]["exchange"].is_null());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dynamic_command_hook_payload_and_progress_grouping_e2e() {
    let root = temp_dir("dynamic-command-hooks-progress");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": {
      "mode": "json",
      "events": "buffered",
      "includeHostState": true,
      "includeBridgeExchange": false
    },
    "interaction": {
      "mode": "auto",
      "defaultStrategy": "use_defaults",
      "timeoutMs": 5000
    },
    "progress": {
      "style": "none",
      "grouping": "operation"
    }
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'sync-user',
          about: 'Sync user info',
          options: [{ long: 'user-id', valueKind: 'string', help: 'User id', required: true }],
          prompt: [{ field: 'user-id', message: 'User id?', kind: 'input', whenMissing: ['user-id'] }],
          handler: async (ctx) => ({
            result: { ok: true, userId: ctx.argv.options['user-id'], exitCode: 0 },
            events: [
              { method: 'event.progress', params: { operationId: 'fetch-user', current: 1, total: 1, message: 'fetch user' } },
              { method: 'event.log', params: { level: 'info', message: 'sync complete' } }
            ]
          })
        }
      ]
    }
  ]
};
"#,
    );

    let prompt_answers = serde_json::json!({
        "user-id": "u-100"
    })
    .to_string();
    let output = run_cli(
        &root,
        &["ops", "sync-user"],
        &[("LANIA_PROMPT_ANSWERS_JSON", prompt_answers.as_str())],
    );
    let json = parse_stdout_json(&output);
    let hooks = json["host_state"]["hooks"]
        .as_array()
        .cloned()
        .expect("hooks array");
    let progress = json["host_state"]["progress"]
        .as_array()
        .cloned()
        .expect("progress array");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["result"]["userId"], "u-100");
    assert!(hooks.iter().any(|item| item["name"] == "onCommandPreInit"));
    assert!(hooks.iter().any(|item| item["name"] == "onArgsParsed"));
    assert!(hooks
        .iter()
        .any(|item| item["name"] == "onInteractionPrompt"));
    assert!(hooks.iter().any(|item| item["name"] == "onPluginApiCall"));
    assert!(hooks.iter().any(|item| item["name"] == "onSuccess"));
    let prompt_hook = hooks
        .iter()
        .rev()
        .find(|item| item["name"] == "onInteractionPrompt")
        .expect("onInteractionPrompt present");
    assert_eq!(
        prompt_hook["payload"]["prompt"]["answers"]["user-id"],
        "u-100"
    );
    assert_eq!(
        progress.first().and_then(|item| item["id"].as_str()),
        Some("fetch-user")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dynamic_command_human_stream_output_e2e() {
    let root = temp_dir("dynamic-command-human-stream");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": {
      "mode": "human",
      "events": "stream",
      "includeHostState": false,
      "includeBridgeExchange": false
    },
    "progress": { "style": "none" }
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'ping',
          handler: async () => ({
            result: { ok: true, value: 'pong', exitCode: 0 },
            events: [{ method: 'event.log', params: { level: 'info', message: 'ping invoked' } }]
          })
        }
      ]
    }
  ]
};
"#,
    );

    let home = isolated_home(&root);
    let output = run_cli(&root, &["ops", "ping"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Bridge Command Completed"));
    assert!(stderr.contains("[event:command.invokeDynamic] ping invoked"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dynamic_command_human_stream_output_zh_e2e() {
    let root = temp_dir("dynamic-command-human-stream-zh");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": {
      "mode": "human",
      "events": "stream",
      "includeHostState": false,
      "includeBridgeExchange": false
    },
    "progress": { "style": "none" }
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'ping',
          handler: async () => ({
            result: { ok: true, value: 'pong', exitCode: 0 },
            events: [{ method: 'event.log', params: { level: 'info', message: 'ping invoked' } }]
          })
        }
      ]
    }
  ]
};
"#,
    );

    let home = isolated_home(&root);
    write_file(
        std::path::Path::new(&home)
            .join(".lania")
            .join("preferences.json"),
        r#"{
  "locale": "zh",
  "outputMode": "json",
  "logTimestamps": false
}
"#,
    );

    let output = run_cli(&root, &["ops", "ping"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("桥接命令执行完成"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_generate_missing_subcommand_zh_e2e() {
    let root = temp_dir("generate-missing-subcommand-zh");
    let home = isolated_home(&root);
    write_file(
        std::path::Path::new(&home)
            .join(".lania")
            .join("preferences.json"),
        r#"{
  "locale": "zh",
  "outputMode": "json",
  "logTimestamps": false
}
"#,
    );

    let output = run_cli(&root, &["generate"], &[("HOME", home.as_str())]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("缺少 generate 子命令"));
    assert!(stdout.contains("缺少 generate 子命令"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn lan_dynamic_command_inline_hook_executes_and_rewrites_argv_e2e() {
    let root = temp_dir("dynamic-command-inline-hook");
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "ui": {
    "output": { "mode": "json", "events": "buffered", "includeHostState": false, "includeBridgeExchange": false },
    "progress": { "style": "none" }
  }
}
"#,
    );
    write_file(
        root.join("lania.schemas.js"),
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'ping',
          hooks: {
            onArgsParsed: [
              (payload) => ({
                ...payload,
                argv: {
                  ...payload.argv,
                  options: { ...(payload.argv?.options ?? {}), foo: 'bar' }
                }
              })
            ]
          },
          handler: async (ctx) => ({ result: { ok: true, foo: ctx.argv.options.foo, exitCode: 0 }, events: [] })
        }
      ]
    }
  ]
};
"#,
    );

    let output = run_cli(&root, &["ops", "ping"], &[]);
    let json = parse_stdout_json(&output);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json["result"]["result"]["foo"], "bar");

    let _ = fs::remove_dir_all(root);
}
