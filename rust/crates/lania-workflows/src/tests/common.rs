//! workflow 测试共享的夹具、样例数据与断言辅助。
//!
//! 关键点：
//! - 包含子进程/环境变量交互
use super::*;

pub(super) static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should work")
        .as_nanos();
    let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "lania-workflow-{name}-{unique}-{}-{counter}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temp dir created");
    path
}

pub(super) fn services(exec: ExecService) -> (WorkflowServices, PathBuf) {
    let root = temp_dir("root");
    (
        WorkflowServices {
            logger: lania_logger::LoggerService::default(),
            prompt: PromptService::default(),
            fs: FsService::default(),
            git: GitService::default(),
            package_manager: PackageManagerService,
            exec,
            tasks: TaskService::default(),
            progress: ProgressService::default(),
            bridge: NodeBridgeClient::new(Default::default()),
            hooks: std::sync::Arc::new(lania_hooks::HookBusImpl::new()),
            hook_cwd: "/tmp".into(),
            hook_trace_id: "trace-test".into(),
            hook_command_handler_id: "workflow-test".into(),
            locale: "en".into(),
        },
        root,
    )
}

pub(super) fn init_git_repo(repo: &Path) {
    std::fs::create_dir_all(repo).expect("repo dir exists");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(repo)
        .output()
        .expect("git init succeeds");
    std::process::Command::new("git")
        .args(["config", "user.name", "Lania Test"])
        .current_dir(repo)
        .output()
        .expect("git user name succeeds");
    std::process::Command::new("git")
        .args(["config", "user.email", "lania@example.com"])
        .current_dir(repo)
        .output()
        .expect("git user email succeeds");
    std::fs::write(repo.join("README.md"), "hello\n").expect("readme written");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .output()
        .expect("git add succeeds");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo)
        .output()
        .expect("git commit succeeds");
}

pub(super) fn release_test_plan(
    repo: &Path,
    state_file: &Path,
    deploy_command: &str,
) -> ReleasePlan {
    ReleasePlan {
        cwd: repo.to_path_buf(),
        profile: ReleaseProfile::WebApp,
        env: Some("prod".into()),
        channel: Some("stable".into()),
        version: None,
        publish: false,
        state_file: state_file.to_path_buf(),
        from_stage: None,
        to_stage: None,
        skip_stages: BTreeSet::new(),
        apply: true,
        dry_run: false,
        verify: ReleaseVerifyConfig::default(),
        versioning: ReleaseVersioningConfig {
            enabled: false,
            ..ReleaseVersioningConfig::default()
        },
        changelog: ReleaseStepConfig::default(),
        artifact: ReleaseStepConfig {
            enabled: true,
            command: Some(
                "if [ -f artifact.count ]; then printf 'rerun\\n' >> artifact.count; else printf 'first\\n' > artifact.count; fi && mkdir -p dist && printf artifact > dist/app.txt".into(),
            ),
        },
        deploy: lania_config::ReleaseDeployConfig {
            provider: "custom".into(),
            command: Some(deploy_command.into()),
        },
        post_check: ReleasePostCheckConfig {
            url: None,
            command: Some("printf ok > post-check.ok".into()),
        },
        git: ReleaseGitConfig {
            commit: false,
            tag: false,
            push: false,
            remote: None,
            branch: None,
        },
        package_manager: PackageManager::Npm,
    }
}

pub(super) fn stage_status(
    snapshot: &ReleaseStateSnapshot,
    stage: ReleaseStage,
) -> ReleaseStageStatus {
    snapshot
        .stages
        .iter()
        .find(|candidate| candidate.stage == stage)
        .map(|candidate| candidate.status)
        .expect("stage exists")
}

pub(super) fn write_contract_config(root: &Path, body: &str) {
    std::fs::write(root.join("lania.contract.yaml"), body).expect("contract config written");
}

pub(super) fn write_module_config(root: &Path, body: &str) {
    std::fs::write(root.join("lania.module.yaml"), body).expect("module config written");
}

pub(super) fn write_proto_schema(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("schema dir created");
    }
    std::fs::write(path, body).expect("proto schema written");
}

pub(super) fn generate_input(root: &Path) -> GenerateApiWorkflowInput {
    GenerateApiWorkflowInput {
        cwd: root.to_path_buf(),
        config_path: None,
        manifest_path: None,
        source_filter: Vec::new(),
        target_filter: Vec::new(),
        entry_filter: Vec::new(),
        dry_run: false,
        check: false,
        clean: false,
        force: false,
        mode: GenerateApiMode::Apply,
    }
}

pub(super) fn generate_module_input(root: &Path) -> GenerateModuleWorkflowInput {
    GenerateModuleWorkflowInput {
        cwd: root.to_path_buf(),
        config_path: None,
        manifest_path: None,
        input_path: None,
        source_filter: Vec::new(),
        target_filter: Vec::new(),
        entry_filter: Vec::new(),
        framework: None,
        main_path: None,
        module_name: None,
        package_name: None,
        dry_run: false,
        check: false,
        clean: false,
        force: false,
        no_inject: false,
        mode: GenerateModuleMode::Apply,
    }
}
