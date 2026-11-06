//! Workflow 与 hooks 系统之间的适配层。
//!
//! workflow 在执行过程中会产生很多“可插入点”：
//! - 文件列表准备好但还没写盘
//! - 依赖列表刚算出来，允许插件改写
//! - 文件写入前/后/冲突时，需要广播事件
//!
//! 这个文件把这些时机统一包装成 hook payload，避免每个 workflow 自己拼 JSON。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lania_fs::{PlannedFile, WriteReport};
use lania_hooks::hook_keys;
use serde_json::{json, Value};

use crate::models::WorkflowServices;

pub(crate) fn base_payload(services: &WorkflowServices, workflow: &str) -> Value {
    // workflow hooks 的 payload 设计成统一骨架：
    // - cwd / traceId / command / workflow 几乎所有 hook 都会用到
    // - 具体 hook 再在这个基础上 merge 自己的额外字段
    json!({
        "cwd": services.hook_cwd.as_str(),
        "traceId": services.hook_trace_id.as_str(),
        "command": { "name": services.hook_command_handler_id.as_str(), "handlerId": services.hook_command_handler_id.as_str() },
        "workflow": { "name": workflow }
    })
}

pub(crate) fn planned_files_payload(target_dir: &Path, files: &[PlannedFile]) -> Value {
    let items = files
        .iter()
        .map(|file| {
            let relative = file
                .path
                .strip_prefix(target_dir)
                .ok()
                .and_then(|path| path.to_str())
                .map(|value| value.to_string());
            json!({
                "path": file.path.display().to_string(),
                "relativePath": relative,
                "content": file.content
            })
        })
        .collect::<Vec<_>>();
    json!({ "files": items })
}

pub(crate) fn apply_files_patch(
    target_dir: &Path,
    payload: &Value,
) -> Result<Option<Vec<PlannedFile>>> {
    // 这里允许 hook 直接返回一组“文件补丁后结果”。
    // 换句话说，hook 不只是旁路观察者，也可以真正改写工作流要写出的文件集。
    let Some(items) = payload.get("files").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut files = Vec::new();
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| anyhow!("hook files item must be object"))?;
        let path = object
            .get("relativePath")
            .and_then(Value::as_str)
            .or_else(|| object.get("path").and_then(Value::as_str))
            .ok_or_else(|| anyhow!("hook files item must include path or relativePath"))?;
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("hook files item must include content"))?;
        let path_buf = PathBuf::from(path);
        let path = if path_buf.is_absolute() {
            path_buf
        } else {
            target_dir.join(path)
        };
        files.push(PlannedFile {
            path,
            content: content.to_string(),
        });
    }
    Ok(Some(files))
}

pub(crate) async fn call_files_prepare(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    files: &mut Vec<PlannedFile>,
) -> Result<()> {
    let payload = base_payload(services, workflow);
    let payload = merge(payload, planned_files_payload(target_dir, files));
    // `call_waterfall` 表示“前一个 hook 的输出会成为后一个 hook 的输入”。
    // 这非常适合文件准备阶段，因为多个 hook 可以在同一份文件列表上逐步改写。
    let out = services
        .hooks
        .call_waterfall(
            format!("workflow:{workflow}"),
            hook_keys::ON_FILES_PREPARE.to_string(),
            payload,
        )
        .await
        .unwrap_or_else(|_| json!({}));
    if let Ok(Some(next)) = apply_files_patch(target_dir, &out) {
        *files = next;
    }
    Ok(())
}

pub(crate) async fn write_files_with_hooks(
    services: &WorkflowServices,
    workflow: &str,
    target_dir: &Path,
    files: &[PlannedFile],
    overwrite: bool,
) -> Result<WriteReport> {
    // 文件真正写盘时不走 waterfall，而是改用 parallel：
    // - before/after/conflict 更偏通知事件
    // - 不希望多个 hook 再次串改真实写盘动作，避免语义混乱
    let mut written = Vec::new();
    let mut conflicts = Vec::new();

    for file in files {
        if file.path.exists() && !overwrite {
            conflicts.push(file.path.clone());
            // 冲突仍然广播 hook，而不是静默跳过：
            // 这样插件/观测层能知道“为什么这个文件最后没写进去”。
            let payload = merge(
                base_payload(services, workflow),
                json!({
                    "file": {
                        "path": file.path.display().to_string(),
                        "relativePath": file.path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                        "stage": "conflict"
                    }
                }),
            );
            services
                .hooks
                .call_parallel(
                    format!("workflow:{workflow}"),
                    hook_keys::ON_FILE_WRITE.to_string(),
                    payload,
                )
                .await
                .ok();
            continue;
        }

        if let Some(parent) = file.path.parent() {
            services
                .fs
                .ensure_dir(parent)
                .with_context(|| format!("failed to ensure dir {}", parent.display()))?;
        }
        // 这里是 per-file before/after hook，而不是整个批次只发一次，
        // 因为插件通常更关心“某个具体文件在写盘前后发生了什么”。

        let before = merge(
            base_payload(services, workflow),
            json!({
                "file": {
                    "path": file.path.display().to_string(),
                    "relativePath": file.path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                    "stage": "before"
                }
            }),
        );
        services
            .hooks
            .call_parallel(
                format!("workflow:{workflow}"),
                hook_keys::ON_FILE_WRITE.to_string(),
                before,
            )
            .await
            .ok();

        match std::fs::write(&file.path, &file.content) {
            Ok(()) => {
                written.push(file.path.clone());
                let after = merge(
                    base_payload(services, workflow),
                    json!({
                        "file": {
                            "path": file.path.display().to_string(),
                            "relativePath": file.path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                            "stage": "after",
                            "ok": true
                        }
                    }),
                );
                services
                    .hooks
                    .call_parallel(
                        format!("workflow:{workflow}"),
                        hook_keys::ON_FILE_WRITE.to_string(),
                        after,
                    )
                    .await
                    .ok();
            }
            Err(error) => {
                let after = merge(
                    base_payload(services, workflow),
                    json!({
                        "file": {
                            "path": file.path.display().to_string(),
                            "relativePath": file.path.strip_prefix(target_dir).ok().and_then(|p| p.to_str()).map(|s| s.to_string()),
                            "stage": "after",
                            "ok": false,
                            "error": { "message": error.to_string() }
                        }
                    }),
                );
                services
                    .hooks
                    .call_parallel(
                        format!("workflow:{workflow}"),
                        hook_keys::ON_FILE_WRITE.to_string(),
                        after,
                    )
                    .await
                    .ok();
                // 真正的 IO 错误直接让 workflow 失败，
                // 不会像业务冲突那样继续收集后面的文件结果。
                return Err(anyhow!(
                    "failed to write file {}: {error}",
                    file.path.display()
                ));
            }
        }
    }

    Ok(WriteReport { written, conflicts })
}

pub(crate) async fn call_template_parse(
    services: &WorkflowServices,
    workflow: &str,
    template: &str,
    context: &Value,
    target_dir: &Path,
    files: &mut Vec<PlannedFile>,
) -> Result<()> {
    let payload = merge(
        base_payload(services, workflow),
        json!({
            "template": { "name": template },
            "context": context
        }),
    );
    let payload = merge(payload, planned_files_payload(target_dir, files));
    let out = services
        .hooks
        .call_waterfall(
            format!("workflow:{workflow}"),
            hook_keys::ON_TEMPLATE_PARSE.to_string(),
            payload,
        )
        .await
        .unwrap_or_else(|_| json!({}));
    if let Ok(Some(next)) = apply_files_patch(target_dir, &out) {
        *files = next;
    }
    Ok(())
}

pub(crate) async fn call_dependencies_modify(
    services: &WorkflowServices,
    workflow: &str,
    manager: &str,
    dependencies: &mut Vec<String>,
    dev_dependencies: &mut Vec<String>,
) -> Result<()> {
    // 依赖列表是典型的“可被多个插件加工”的数据，因此也适合 waterfall。
    let payload = merge(
        base_payload(services, workflow),
        json!({
            "dependencies": {
                "manager": manager,
                "dependencies": dependencies,
                "devDependencies": dev_dependencies
            }
        }),
    );
    let out = services
        .hooks
        .call_waterfall(
            format!("workflow:{workflow}"),
            hook_keys::ON_DEPENDENCIES_MODIFY.to_string(),
            payload,
        )
        .await
        .unwrap_or_else(|_| json!({}));
    if let Some(items) = out
        .get("dependencies")
        .and_then(|v| v.get("dependencies"))
        .and_then(Value::as_array)
    {
        *dependencies = items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }
    if let Some(items) = out
        .get("dependencies")
        .and_then(|v| v.get("devDependencies"))
        .and_then(Value::as_array)
    {
        *dev_dependencies = items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }
    Ok(())
}

pub(crate) async fn call_shell_command_before(
    services: &WorkflowServices,
    workflow: &str,
    cwd: &Path,
    program: &str,
    args: &[String],
) {
    let payload = merge(
        base_payload(services, workflow),
        json!({
            "shell": {
                "stage": "before",
                "cwd": cwd.display().to_string(),
                "program": program,
                "args": args
            }
        }),
    );
    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            hook_keys::ON_SHELL_COMMAND.to_string(),
            payload,
        )
        .await
        .ok();
}

pub(crate) async fn call_shell_command_after(
    services: &WorkflowServices,
    workflow: &str,
    cwd: &Path,
    exit_code: i32,
) {
    let payload = merge(
        base_payload(services, workflow),
        json!({
            "shell": {
                "stage": "after",
                "cwd": cwd.display().to_string(),
                "exitCode": exit_code
            }
        }),
    );
    services
        .hooks
        .call_parallel(
            format!("workflow:{workflow}"),
            hook_keys::ON_SHELL_COMMAND.to_string(),
            payload,
        )
        .await
        .ok();
}

pub(crate) fn merge(mut base: Value, extra: Value) -> Value {
    // 这里只做一层对象级 merge，不做深度递归合并。
    // 对 hook payload 来说通常已经够用，而且语义更可预测。
    let Some(base_obj) = base.as_object_mut() else {
        return extra;
    };
    if let Some(extra_obj) = extra.as_object() {
        for (k, v) in extra_obj {
            base_obj.insert(k.clone(), v.clone());
        }
    }
    base
}
