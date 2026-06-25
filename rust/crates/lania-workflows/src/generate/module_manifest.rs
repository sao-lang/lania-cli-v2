//! 读取与更新模块 manifest，跟踪已生成模块的元数据。
//!
//! 这份 manifest（锁文件）解决的是“增量生成/清理”问题：
//! - 生成器每次运行都会产出一批计划文件（GeneratedContractPlan）
//! - 但磁盘上可能已经有旧的生成文件，甚至还有用户自己写的同名文件
//! - 没有 manifest，就很难区分“哪些文件是生成器认领的（managed）”和“哪些是用户自管的”
//!
//! 所以这里的核心概念是 managed paths：
//! - 如果一个文件已存在，但不在旧 manifest 的 managed paths 里，默认视为冲突，不会覆盖
//! - clean 模式会基于旧 manifest 找到 stale 输出，再按策略删除
//!
//! 关键点：
//! - 包含序列化/反序列化与 JSON 结构约定
use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use lania_format::{FormatMode, FormatOptions, FormatService};
use lania_fs::PlannedFile;

use crate::generate_module_inject::resolve_against_cwd;
use crate::generate_schema::stable_hash;
use crate::generate_types::{
    CompiledModuleEntry, ContractPlanSummary, GeneratedContractPlan, ModuleConfig, ModuleManifest,
    ModuleManifestEntry, ModuleOutputConfig,
};
use crate::models::{GenerateModuleMode, GenerateModuleWorkflowInput, WorkflowServices};
use crate::workflow_hooks::{call_files_prepare, write_files_with_hooks};

pub(crate) fn append_generate_module_notes(
    notes: &mut Vec<String>,
    summary: &ContractPlanSummary,
    input: &GenerateModuleWorkflowInput,
    configured_entries: usize,
) {
    // notes 是给最终 CLI 输出看的“人类摘要”，
    // 这里集中拼装，而不是散落在 workflow 各处，便于统一维护文案。
    notes.push(format!("configured entries: {configured_entries}"));
    notes.push(format!(
        "plan summary: {} write, {} unchanged, {} conflicts, {} stale",
        summary.to_write.len(),
        summary.unchanged.len(),
        summary.conflicts.len(),
        summary.stale.len()
    ));
    if matches!(input.mode, GenerateModuleMode::Plan) || input.dry_run {
        notes.push("dry-run: no files were written".into());
    }
    if matches!(input.mode, GenerateModuleMode::Diff) {
        notes.push("diff mode: compared module manifest and current generation plan".into());
    }
    if input.check {
        if !summary.to_write.is_empty()
            || !summary.stale.is_empty()
            || !summary.conflicts.is_empty()
        {
            notes.push("drift detected: generated module outputs are not up to date".into());
        } else {
            notes.push("check passed: generated module outputs are up to date".into());
        }
    }
    if !summary.unchanged.is_empty() {
        notes.push(format!(
            "incremental skip: {} unchanged module files reused",
            summary.unchanged.len()
        ));
    }
    if !summary.conflicts.is_empty() {
        notes.push(format!(
            "conflicts detected: {} unmanaged files require --force",
            summary.conflicts.len()
        ));
    }
    if input.clean && !summary.stale.is_empty() {
        notes.push(format!(
            "clean mode: {} stale generated files will be removed",
            summary.stale.len()
        ));
    }
}

pub(crate) fn render_generate_module_command_plan(
    config_path: &Path,
    input: &GenerateModuleWorkflowInput,
) -> Vec<String> {
    // 这里渲染的是“等价 CLI 命令行”，用于最终输出或调试回放。
    // 注意：它不是为了真的再执行一遍，而是为了让用户清楚“这次 workflow 用了哪些参数”。
    let mut command = vec!["generate".into(), "module".into()];
    match input.mode {
        GenerateModuleMode::Plan => command.push("plan".into()),
        GenerateModuleMode::Diff => command.push("diff".into()),
        GenerateModuleMode::Init => command.push("init".into()),
        GenerateModuleMode::Apply => {}
    }
    command.push("--config".into());
    command.push(config_path.display().to_string());
    if let Some(value) = &input.input_path {
        command.push("--input".into());
        command.push(value.clone());
    }
    if !input.source_filter.is_empty() {
        command.push("--source".into());
        command.push(input.source_filter.join(","));
    }
    if !input.target_filter.is_empty() {
        command.push("--target".into());
        command.push(input.target_filter.join(","));
    }
    if !input.entry_filter.is_empty() {
        command.push("--entry".into());
        command.push(input.entry_filter.join(","));
    }
    if let Some(value) = &input.framework {
        command.push("--framework".into());
        command.push(value.clone());
    }
    if let Some(value) = &input.main_path {
        command.push("--main".into());
        command.push(value.clone());
    }
    if let Some(value) = &input.module_name {
        command.push("--module-name".into());
        command.push(value.clone());
    }
    if let Some(value) = &input.package_name {
        command.push("--package".into());
        command.push(value.clone());
    }
    if let Some(value) = &input.manifest_path {
        command.push("--manifest".into());
        command.push(value.clone());
    }
    if input.check {
        command.push("--check".into());
    }
    if input.clean {
        command.push("--clean".into());
    }
    if input.dry_run {
        command.push("--dry-run".into());
    }
    if input.force {
        command.push("--force".into());
    }
    if input.no_inject {
        command.push("--no-inject".into());
    }
    command
}

pub(crate) fn default_module_config() -> &'static str {
    // init 模式会写出这份默认配置，保证用户“零配置”也能跑通一次 generate-module。
    // 它同时充当文档：展示 schema path、targets、output 和 inject 这些字段怎么配。
    "version: 1\n\nframework:\n  name: lania-g\n  language: go\n  main: main.go\n\ninputs:\n  - name: greeter\n    source: protobuf\n    path: schemas/proto\n    include:\n      - \"**/*.proto\"\n    # Optional per-input targets override.\n    # When omitted, this input falls back to top-level `targets`.\n    targets:\n      - grpc\n      - http\n\ntargets:\n  - kind: grpc\n  - kind: http\n\noutput:\n  root: generated/lania\n  moduleDir: generated/lania/modules\n  adapterDir: generated/lania/adapters\n  contractDir: generated/lania/contracts\n  # Optional: for HTTP output, write `bootstrap.gen.go` into this root\n  # and place grouped handler files under `<httpRootDir>/<handler_path>/...`.\n  # httpRootDir: generated/http\n  # Optional: for gRPC output, write `bootstrap.gen.go` into this root\n  # and place grouped service files under `<grpcRootDir>/<service>/...`.\n  # grpcRootDir: generated/grpc\n  manifest: .lania/module-gen.lock.json\n\ninject:\n  enabled: true\n  targetMain: main.go\n  strategy: marker\n  marker:\n    start: \"lania:modules:start\"\n    end: \"lania:modules:end\"\n"
}

pub(crate) fn resolve_module_config_path(input: &GenerateModuleWorkflowInput) -> PathBuf {
    // config path 的解析统一使用 `resolve_against_cwd`：
    // - 传绝对路径：直接用
    // - 传相对路径：相对 cwd 拼接
    let raw = input.config_path.as_deref().unwrap_or("lania.module.yaml");
    resolve_against_cwd(&input.cwd, raw)
}

pub(crate) fn resolve_module_manifest_path(
    input: &GenerateModuleWorkflowInput,
    config_dir: &Path,
    output: &ModuleOutputConfig,
) -> PathBuf {
    // manifest path 的优先级：
    // 1) CLI `--manifest`
    // 2) config.output.manifest
    // 3) 默认 `.lania/module-gen.lock.json`
    let raw = input
        .manifest_path
        .as_deref()
        .or(output.manifest.as_deref())
        .unwrap_or(".lania/module-gen.lock.json");
    resolve_against_cwd(config_dir, raw)
}

pub(crate) fn validate_module_config(config: &ModuleConfig) -> Result<()> {
    // 这里做的是“结构级校验”，先把明显不合法的配置挡在生成流程之前，
    // 避免后面走到渲染阶段才以更晦涩的方式失败。
    if config.version != 1 {
        return Err(anyhow!(
            "unsupported module config version: {}",
            config.version
        ));
    }
    if config.inputs.is_empty() {
        return Err(anyhow!("module config must contain at least one input"));
    }
    if config.framework.name.trim().is_empty() {
        return Err(anyhow!("module config must declare framework.name"));
    }
    if let Some(strategy) = config
        .inject
        .as_ref()
        .and_then(|inject| inject.strategy.as_deref())
    {
        if strategy != "marker" && strategy != "ast" {
            return Err(anyhow!("unsupported injection strategy: {strategy}"));
        }
    }
    Ok(())
}

pub(crate) fn load_module_manifest(
    path: &Path,
    config_path: &Path,
    framework: &str,
) -> Result<ModuleManifest> {
    // manifest 不存在时不会报错，而是返回一个空壳默认值：
    // 这让首次生成和后续增量生成可以共用同一套读取逻辑。
    if !path.exists() {
        return Ok(ModuleManifest {
            version: 1,
            config_path: config_path.display().to_string(),
            framework: framework.to_string(),
            shared_outputs: Vec::new(),
            entries: BTreeMap::new(),
        });
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read module manifest {}", path.display()))?;
    let manifest: ModuleManifest = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
    Ok(manifest)
}

pub(crate) async fn write_module_manifest(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    path: &Path,
    manifest: &ModuleManifest,
) -> Result<()> {
    // manifest 虽然只是元数据文件，但仍然走“格式化 + files_prepare + hooks + write”全管线，
    // 这样它和其它生成文件的行为保持一致。
    let mut content = serde_json::to_string_pretty(manifest)?;
    content.push('\n');
    let formatter = FormatService;
    let mut files = vec![PlannedFile {
        path: path.to_path_buf(),
        content,
    }];
    let root_dir = path.parent().map(Path::to_path_buf);
    let _format_report = formatter.format_planned_files(
        &services.exec,
        &mut files,
        &FormatOptions {
            enabled: true,
            mode: FormatMode::BestEffort,
            root_dir,
        },
    )?;
    let content = files.pop().map(|file| file.content).unwrap_or_default();
    let mut planned = vec![PlannedFile {
        path: path.to_path_buf(),
        content,
    }];
    call_files_prepare(services, workflow, target_dir, &mut planned).await?;
    let _report = write_files_with_hooks(services, workflow, target_dir, &planned, true).await?;
    Ok(())
}

pub(crate) fn update_module_manifest(
    manifest: &mut ModuleManifest,
    entries: &[CompiledModuleEntry],
    plans: &[GeneratedContractPlan],
    config_path: &Path,
    framework: &str,
) {
    // manifest 记录的不是“这次写了什么”，而是“生成器当前认领了哪些输出”。
    // 后续 diff/clean/conflict 判断都会依赖这份记录。
    manifest.version = 1;
    manifest.config_path = config_path.display().to_string();
    manifest.framework = framework.to_string();
    manifest.shared_outputs = plans
        .iter()
        // shared_outputs 指的是“不属于某个 entry 的全局输出”，例如 registry 文件。
        // `main.go` 的改写属于“注入行为”，不是生成器认领的稳定输出，因此排除。
        .filter(|plan| plan.owner.is_none() && !is_main_go_path(&plan.path))
        .map(|plan| plan.path.display().to_string())
        .collect();
    for entry in entries {
        let outputs = plans
            .iter()
            .filter(|plan| plan.owner.as_deref() == Some(entry.name.as_str()))
            .map(|plan| plan.path.display().to_string())
            .collect::<Vec<_>>();
        // 这两个 hash 用于 diff/check：
        // - input_hash：输入 schema 内容变化了吗
        // - ir_hash：解析出来的 IR 变化了吗（有助于定位“parser/normalize 规则变化”）
        //
        // 它们不是安全校验，不用于防篡改，只用于快速比较。
        let source_snapshot = entry
            .input_paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n--input--\n");
        let ir_snapshot = format!("{:#?}", entry.ir);
        manifest.entries.insert(
            entry.name.clone(),
            ModuleManifestEntry {
                source_kind: entry.source_kind.clone(),
                targets: entry.targets.clone(),
                input_hash: stable_hash(&source_snapshot),
                ir_hash: stable_hash(&ir_snapshot),
                outputs,
            },
        );
    }
}

pub(crate) fn module_manifest_managed_paths(
    manifest: &ModuleManifest,
    selected_entries: &[CompiledModuleEntry],
    scoped: bool,
) -> BTreeSet<PathBuf> {
    // `scoped=true` 时只返回“当前选中 entry”相关的路径，
    // 这能避免用户只生成一个 entry 时误伤其它 entry 的历史输出。
    // shared_outputs 永远算 managed（除非用户手动删掉，clean 才会补齐逻辑）。
    let mut paths = manifest
        .shared_outputs
        .iter()
        .map(PathBuf::from)
        .collect::<BTreeSet<_>>();
    let selected_names = selected_entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    for (entry_name, entry) in &manifest.entries {
        if scoped && !selected_names.contains(entry_name.as_str()) {
            continue;
        }
        for output in &entry.outputs {
            paths.insert(PathBuf::from(output));
        }
    }
    paths
}

pub(crate) fn has_generate_module_filters(input: &GenerateModuleWorkflowInput) -> bool {
    input.input_path.is_some()
        || !input.entry_filter.is_empty()
        || !input.source_filter.is_empty()
        || !input.target_filter.is_empty()
}

pub(crate) fn module_safe_overwrite_paths(plans: &[GeneratedContractPlan]) -> BTreeSet<PathBuf> {
    // “安全覆盖”路径：目前只包含 main.go。
    // 因为 main.go 的改写是“注入点”行为，且它受 marker 保护，
    // 不会无条件覆盖用户代码（看不到 marker 就不会生成对应 plan）。
    plans
        .iter()
        .filter(|plan| is_main_go_path(&plan.path))
        .map(|plan| plan.path.clone())
        .collect()
}

pub(crate) fn is_main_go_path(path: &Path) -> bool {
    // 对 generate-module 来说，main.go 是一个“特殊文件”：
    // - 它属于用户工程入口文件
    // - 生成器只能在非常明确的条件下改写（marker/ast strategy）
    // 因此这里单独抽出来做判断。
    path.file_name().and_then(|value| value.to_str()) == Some("main.go")
}
