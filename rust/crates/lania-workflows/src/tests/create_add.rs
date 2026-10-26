//! create 与 add 相关 workflow 的回归测试。
//!
//! 关键点：
//! - 包含异步/超时/取消等控制流
use super::*;

#[tokio::test]
async fn create_workflow_writes_template_files() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: None,
                project_name: Some("demo-app".into()),
                template: Some("spa-react".into()),
                package_manager: Some("npm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("create workflow succeeds");

    assert!(root.join("demo-app/src/main.tsx").exists());
    assert_eq!(result.workflow, "create");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_detects_conflicts() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = AddWorkflow;
    let target = root.join("src").join("components");
    std::fs::create_dir_all(&target).expect("target dir exists");
    std::fs::write(target.join("Button.tsx"), "existing").expect("seed conflict");

    let result = workflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("Button".into()),
                template: Some("rfc".into()),
                target: Some("src/components".into()),
                force: false,
            },
        )
        .await
        .expect("add workflow succeeds");

    assert!(!result.conflicts.is_empty());
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("framework: generic")));
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("generator: component")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_generates_nest_resource_and_registers_module() {
    let (services, root) = services(ExecService::dry_run());
    // 框架探测：`nest-cli.json` 存在即认定为 nestjs。
    std::fs::write(root.join("nest-cli.json"), "{}").expect("nest cli marker");
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(
        root.join("src/app.module.ts"),
        "// <lania:imports>\n@Module({})\nexport class AppModule {}\n",
    )
    .expect("app module");

    let result = AddWorkflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("users".into()),
                template: Some("resource".into()),
                target: None,
                force: false,
            },
        )
        .await
        .expect("nest resource generation succeeds");
    assert_eq!(
        result
            .notes
            .iter()
            .filter(|note| note.contains("generator: resource"))
            .count(),
        1
    );

    for expected in [
        "src/users/users.module.ts",
        "src/users/users.controller.ts",
        "src/users/users.service.ts",
        "src/users/dto/create-user.dto.ts",
        "src/users/dto/update-user.dto.ts",
        "src/users/entities/user.entity.ts",
        "src/users/users.controller.spec.ts",
    ] {
        assert!(root.join(expected).exists(), "missing {expected}");
    }
    let app_module = std::fs::read_to_string(root.join("src/app.module.ts")).expect("app module");
    assert!(app_module.contains("import { UsersModule } from './users/users.module';"));
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("framework: nestjs")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_without_framework_only_offers_generic_generators() {
    let (services, root) = services(ExecService::dry_run());
    let result = AddWorkflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("button".into()),
                template: Some("component".into()),
                target: Some("src".into()),
                force: false,
            },
        )
        .await
        .expect("generic component generation succeeds");
    assert!(root.join("src/components/Button.tsx").exists());
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("framework: generic")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unique_path_keeps_extension_and_handles_dotfiles() {
    let root = temp_dir("unique-path");
    std::fs::write(root.join("Button.tsx"), "x").expect("seed conflict");
    assert_eq!(
        super::super::create::add_workflow::unique_path(&root.join("Button.tsx")),
        root.join("Button-1.tsx")
    );
    std::fs::write(root.join(".gitignore"), "x").expect("seed dotfile conflict");
    assert_eq!(
        super::super::create::add_workflow::unique_path(&root.join(".gitignore")),
        root.join(".gitignore-1")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_skips_conflicts_without_force_and_reports_them() {
    let (services, root) = services(ExecService::dry_run());
    let target = root.join("src").join("components");
    std::fs::create_dir_all(&target).expect("target dir exists");
    std::fs::write(target.join("Button.tsx"), "hand-written").expect("seed conflict");

    // 非交互环境（测试进程 stdin 不是 TTY）必须保持「跳过 + 计入 conflicts」，
    // 而不是阻塞等待输入。
    let result = AddWorkflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("Button".into()),
                template: Some("component".into()),
                target: Some("src/components".into()),
                force: false,
            },
        )
        .await
        .expect("add workflow succeeds");

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(
        std::fs::read_to_string(target.join("Button.tsx")).expect("existing file readable"),
        "hand-written",
        "existing file must stay untouched when the conflict is skipped"
    );
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("skipped existing src/components/Button.tsx")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_force_overwrites_conflicts() {
    let (services, root) = services(ExecService::dry_run());
    let target = root.join("src").join("components");
    std::fs::create_dir_all(&target).expect("target dir exists");
    std::fs::write(target.join("Button.tsx"), "hand-written").expect("seed conflict");

    let result = AddWorkflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("Button".into()),
                template: Some("component".into()),
                target: Some("src/components".into()),
                force: true,
            },
        )
        .await
        .expect("add workflow succeeds");

    assert!(result.conflicts.is_empty());
    let content =
        std::fs::read_to_string(target.join("Button.tsx")).expect("generated file readable");
    assert!(content.contains("export function Button"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn add_workflow_accepts_legacy_absolute_looking_target() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = AddWorkflow;

    let result = workflow
        .run(
            &services,
            AddWorkflowInput {
                cwd: root.clone(),
                name: Some("Button".into()),
                template: Some("rfc".into()),
                target: Some("/src/components".into()),
                force: false,
            },
        )
        .await
        .expect("legacy-looking absolute target should normalize");

    assert!(root.join("src/components/Button.tsx").exists());
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("normalized target: src/components")));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_uses_bridge_template_for_toolkit() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: None,
                project_name: Some("demo-kit".into()),
                template: Some("toolkit".into()),
                package_manager: Some("pnpm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("create workflow succeeds");

    assert!(root.join("demo-kit/src/index.ts").exists());
    assert!(!result.bridge_steps.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_discovers_bridge_only_templates() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: None,
                project_name: Some("demo-kit-workspace".into()),
                template: Some("toolkit-monorepo".into()),
                package_manager: Some("pnpm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("create workflow succeeds");

    assert!(root
        .join("demo-kit-workspace/packages/core/src/index.ts")
        .exists());
    assert!(!result.bridge_steps.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_preview_does_not_write_files() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: None,
                project_name: Some("preview-app".into()),
                template: Some("toolkit".into()),
                package_manager: Some("pnpm".into()),
                lint_tools: None,
                language: None,
                init_git: true,
                skip_install: false,
                skip_install_specified: false,
                dry_run: true,
                preview: true,
            },
        )
        .await
        .expect("create preview succeeds");

    assert_eq!(result.state, WorkflowState::Planned);
    assert!(!root.join("preview-app").exists());
    assert!(result.written_files.is_empty());
    assert!(result
        .prompts
        .get("preview")
        .and_then(Value::as_bool)
        .unwrap_or(false));
    assert!(result
        .notes
        .iter()
        .any(|note| note.contains("template preview")));
    assert!(!result.command_plans.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_initializes_git_repository() {
    let (services, root) = services(ExecService::new(false));
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: None,
                project_name: Some("git-app".into()),
                template: Some("spa-react".into()),
                package_manager: Some("npm".into()),
                lint_tools: None,
                language: None,
                init_git: true,
                skip_install: true,
                skip_install_specified: true,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("create workflow succeeds");

    let target = root.join("git-app");
    assert!(target.join(".git").exists());
    assert!(result
        .command_plans
        .iter()
        .any(|plan| plan == &vec!["git".to_string(), "init".to_string(),]));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_rejects_non_empty_current_directory_when_path_is_dot() {
    let (services, root) = services(ExecService::dry_run());
    std::fs::write(root.join("README.md"), "seed\n").expect("seed file written");
    let workflow = CreateWorkflow;
    let error = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: Some(".".into()),
                project_name: Some("demo-app".into()),
                template: Some("spa-react".into()),
                package_manager: Some("npm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect_err("non-empty current directory should fail");

    assert!(error.to_string().contains("current directory is not empty"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_allows_child_directory_when_root_is_non_empty() {
    let (services, root) = services(ExecService::dry_run());
    std::fs::write(root.join("README.md"), "seed\n").expect("seed file written");
    let workflow = CreateWorkflow;
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: Some("foo".into()),
                project_name: Some("demo-app".into()),
                template: Some("spa-react".into()),
                package_manager: Some("npm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("child directory create succeeds");

    assert!(root.join("foo/src/main.tsx").exists());
    assert_eq!(result.target_dir, root.join("foo").display().to_string());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn create_workflow_uses_current_directory_name_for_dot_path() {
    let (services, root) = services(ExecService::dry_run());
    let workflow = CreateWorkflow;
    let expected_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temp dir name")
        .to_string();
    let result = workflow
        .run(
            &services,
            CreateWorkflowInput {
                cwd: root.clone(),
                path: Some(".".into()),
                project_name: None,
                template: Some("spa-react".into()),
                package_manager: Some("npm".into()),
                lint_tools: None,
                language: None,
                init_git: false,
                skip_install: false,
                skip_install_specified: false,
                dry_run: false,
                preview: false,
            },
        )
        .await
        .expect("create in current directory succeeds");

    assert_eq!(result.target_dir, root.display().to_string());
    assert_eq!(
        result
            .prompts
            .get("projectName")
            .and_then(serde_json::Value::as_str),
        Some(expected_name.as_str())
    );
    assert!(root.join("src/main.tsx").exists());
    let package_json =
        std::fs::read_to_string(root.join("package.json")).expect("package.json readable");
    assert!(package_json.contains(&format!("\"name\": \"{expected_name}\"")));
    let _ = std::fs::remove_dir_all(root);
}
