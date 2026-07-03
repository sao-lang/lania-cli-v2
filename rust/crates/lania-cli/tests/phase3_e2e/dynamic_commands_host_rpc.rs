//! Dynamic Commands + Host RPC 全链路 E2E（development 模式）。
//!
//! 覆盖以下场景：
//! - development 模式下 resolveDynamic + invokeDynamic 执行自定义 workflow
//! - 自定义 workflow 通过 `ctx.tools.fs.*` 回调 Rust 宿主能力（Host RPC）
//! - 自定义 workflow 通过 `ctx.tools.exec.*` 回调 Rust 宿主能力（Host RPC）
//! - 自定义 workflow 通过 `ctx.tools.log.*` 输出宿主日志
//! - `ctx.runtime.mode` 在 development 模式下正确标识
//! - `ctx.argv` 正确传递选项参数
//! - runtimeCommands mount 机制正确工作
use super::common::*;
use super::*;

fn setup_bridge_env(_root: &Path) -> Vec<(String, String)> {
    ensure_node_bridge_dist();
    let bridge_dir = node_bridge_package_dir();
    vec![(
        "LANIA_NODE_BRIDGE_DIR".to_string(),
        bridge_dir.to_str().expect("bridge dir path").to_string(),
    )]
}

fn write_dynamic_config(root: &Path, schema_content: &str) {
    write_file(
        root.join("lan.config.json"),
        r#"{
  "extensions": { "dynamicCommands": true },
  "schema": {
    "entry": "./lania.schemas.js"
  }
}
"#,
    );
    write_file(root.join("lania.schemas.js"), schema_content);
}

#[cfg(unix)]
#[test]
fn lan_dynamic_command_invoke_host_rpc_fs_in_development_mode() {
    let root = temp_dir("dynamic-dev-host-rpc-fs");
    let env = setup_bridge_env(&root);

    write_dynamic_config(
        &root,
        r#"export default {
  commands: [
    {
      name: 'write-read-report',
      about: 'Write and read a report via Host RPC fs tools',
      options: [
        { long: 'message', valueKind: 'string', help: 'Message to write', required: true },
        { long: 'filename', valueKind: 'string', help: 'Output filename', required: true }
      ],
      workflow: 'writeReadReport'
    }
  ],
  workflows: {
    async writeReadReport(ctx) {
      const message = String(ctx.argv.options.message ?? 'default');
      const filename = String(ctx.argv.options.filename ?? 'report.txt');
      await ctx.tools.fs.writeFile(filename, `message: ${message}\nmode: ${ctx.runtime.mode}\ncwd: ${ctx.cwd}\n`);
      const content = await ctx.tools.fs.readFile(filename);
      await ctx.tools.log.success(`report written: ${filename}`);
      return {
        result: {
          content,
          mode: ctx.runtime.mode,
          invocationCwd: ctx.runtime.invocationCwd,
          workspaceRoot: ctx.runtime.workspaceRoot,
          productRoot: ctx.runtime.productRoot,
          exitCode: 0
        }
      };
    }
  }
};
"#,
    );

    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let output = run_cli(
        &root,
        &["write-read-report", "--message", "hello-host-rpc", "--filename", ".lania/test-report.txt"],
        &env_refs,
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // 验证文件已通过 Host RPC 写入
    let report_path = root.join(".lania/test-report.txt");
    assert!(report_path.exists(), "report file should exist: {}", report_path.display());
    let report_content = fs::read_to_string(&report_path).expect("report readable");
    assert!(report_content.contains("message: hello-host-rpc"));
    assert!(report_content.contains("mode: development"));

    // 验证 stdout JSON 包含结果
    let json = parse_stdout_json(&output);
    let payload = &json["exchange"]["response"]["result"]["result"]["result"];
    assert_eq!(payload["mode"], "development");
    assert!(payload["content"].as_str().unwrap_or_default().contains("hello-host-rpc"));

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_dynamic_command_invoke_host_rpc_exec_in_development_mode() {
    let root = temp_dir("dynamic-dev-host-rpc-exec");
    let env = setup_bridge_env(&root);

    write_dynamic_config(
        &root,
        r#"export default {
  commands: [
    {
      name: 'run-echo',
      about: 'Run a shell command via Host RPC exec tools',
      options: [
        { long: 'text', valueKind: 'string', help: 'Text to echo', required: true }
      ],
      workflow: 'runEcho'
    }
  ],
  workflows: {
    async runEcho(ctx) {
      const text = String(ctx.argv.options.text ?? '');
      await ctx.tools.fs.writeFile('.lania/exec-input.txt', text);
      const result = await ctx.tools.exec.run('node', ['-e', `console.log(require('fs').readFileSync('.lania/exec-input.txt', 'utf8').trim())`], { cwd: ctx.cwd });
      await ctx.tools.log.info(`exec exit code: ${result.exitCode}`);
      return {
        result: {
          stdout: result.stdout,
          exitCode: result.exitCode,
          mode: ctx.runtime.mode
        }
      };
    }
  }
};
"#,
    );

    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let output = run_cli(
        &root,
        &["run-echo", "--text", "hello-exec"],
        &env_refs,
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let json = parse_stdout_json(&output);
    let payload = &json["exchange"]["response"]["result"]["result"]["result"];
    assert_eq!(payload["mode"], "development");
    assert_eq!(payload["exitCode"], 0);
    assert!(payload["stdout"].as_str().unwrap_or_default().contains("hello-exec"));

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_dynamic_command_invoke_host_rpc_log_in_development_mode() {
    let root = temp_dir("dynamic-dev-host-rpc-log");
    let env = setup_bridge_env(&root);

    write_dynamic_config(
        &root,
        r#"export default {
  commands: [
    {
      name: 'test-log',
      about: 'Test log tools via Host RPC',
      options: [
        { long: 'level', valueKind: 'string', help: 'Log level', required: true }
      ],
      workflow: 'testLog'
    }
  ],
  workflows: {
    async testLog(ctx) {
      const level = String(ctx.argv.options.level ?? 'info');
      if (level === 'info') await ctx.tools.log.info('test info message');
      else if (level === 'warn') await ctx.tools.log.warn('test warn message');
      else if (level === 'error') await ctx.tools.log.error('test error message');
      else if (level === 'success') await ctx.tools.log.success('test success message');
      return {
        result: { logged: level, exitCode: 0 }
      };
    }
  }
};
"#,
    );

    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let output = run_cli(
        &root,
        &["test-log", "--level", "info"],
        &env_refs,
    );
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("test info message"), "stderr should contain log message: {}", stderr);

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn lan_dynamic_command_with_options_and_subcommand_mount() {
    let root = temp_dir("dynamic-dev-options-mount");
    let env = setup_bridge_env(&root);

    write_dynamic_config(
        &root,
        r#"export default {
  runtimeCommands: [
    {
      mount: 'ops',
      commands: [
        {
          name: 'greet',
          about: 'Greet someone',
          options: [
            { long: 'name', valueKind: 'string', help: 'Name to greet', required: true },
            { long: 'shout', valueKind: 'bool', help: 'Shout the greeting' }
          ],
          workflow: 'greet'
        }
      ]
    }
  ],
  workflows: {
    async greet(ctx) {
      let greeting = `Hello, ${ctx.argv.options.name ?? 'world'}!`;
      if (ctx.argv.options.shout) greeting = greeting.toUpperCase();
      return {
        result: { greeting, exitCode: 0 }
      };
    }
  }
};
"#,
    );

    let env_refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let output = run_cli(
        &root,
        &["ops", "greet", "--name", "Lania", "--shout"],
        &env_refs,
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let json = parse_stdout_json(&output);
    let greeting = &json["exchange"]["response"]["result"]["result"]["result"]["greeting"];
    assert_eq!(greeting, "HELLO, LANIA!");

    let _ = fs::remove_dir_all(root);
}
