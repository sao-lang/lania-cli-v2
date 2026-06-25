use std::path::Path;

use anyhow::{anyhow, Result};
use lania_git::{GitCommitLogOptions, GitMergeOptions, GitRebaseOptions, GitRevertOptions};
use serde_json::{json, Value};

use super::{
    command_to_json, payload_bool, payload_cwd, payload_optional_str, payload_required_str,
    payload_strings, HostPayload, HostRpcAdapter, HostRpcResponse,
};

/// git rpc 之所以仍然保持为“一个大模块”，是因为它覆盖的能力面最广。
///
/// 这里按“仓库关注点”分组 handler（repo/remote/branch/stage&history/identity&plan），
/// 这样后续继续拆分时可以逐步把子域迁移出去，而不需要改动稳定的 RPC 方法名与返回结构。
pub(super) fn handle_git_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<HostRpcResponse> {
    if let Some(response) = handle_repository_domain(adapter, method, payload)? {
        return Ok(response);
    }
    if let Some(response) = handle_remote_domain(adapter, method, payload)? {
        return Ok(response);
    }
    if let Some(response) = handle_branch_domain(adapter, method, payload)? {
        return Ok(response);
    }
    if let Some(response) = handle_stage_and_history_domain(adapter, method, payload)? {
        return Ok(response);
    }
    if let Some(response) = handle_identity_and_plan_domain(adapter, method, payload)? {
        return Ok(response);
    }
    Err(anyhow!("unsupported host rpc method: {method}"))
}

/// 仓库级查询与工作区状态探测。
///
/// 该分组负责：
/// - repo 可用性/初始化/状态检查
/// - workspace 级 dirty-state/changed files 等辅助
/// 输入保持以 `cwd` 为中心，返回简单 ack 或序列化后的 git DTO。
fn handle_repository_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<Option<HostRpcResponse>> {
    let response = match method {
        "host.git.status" | "host.git.git.status" | "host.git.workspace.status" => Some((
            serde_json::to_value(adapter.git.status(payload_cwd(payload))?)?,
            Vec::new(),
        )),
        "host.git.git.init" => {
            adapter.git.init(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.git.isInstalled" => Some((
            json!({ "installed": adapter.git.is_installed() }),
            Vec::new(),
        )),
        "host.git.git.version" => Some((json!({ "version": adapter.git.version()? }), Vec::new())),
        "host.git.git.isInit" => Some((
            json!({ "isInit": adapter.git.is_init(payload_cwd(payload)) }),
            Vec::new(),
        )),
        "host.git.git.clone" => {
            let cwd = payload_cwd(payload);
            let repo_url = payload_required_str(payload, "repoUrl", method)?;
            let target_dir = payload_optional_str(payload, "targetDir");
            adapter
                .git
                .clone_repo(cwd, &repo_url, target_dir.as_deref())?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.isClean" | "host.git.workspace.isClean" => Some((
            json!({ "isClean": adapter.git.workspace_is_clean(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.changedFiles" | "host.git.workspace.changedFiles" => Some((
            json!({ "files": adapter.git.workspace_changed_files(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.workspace.statusPorcelain" => Some((
            json!({ "lines": adapter.git.status_porcelain(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.workspace.hasChanges" => Some((
            json!({ "hasChanges": adapter.git.has_working_tree_changes(payload_cwd(payload))? }),
            Vec::new(),
        )),
        _ => None,
    };
    Ok(response)
}

/// Remote-related queries and mutating sync operations.
///
/// All methods require explicit remote/branch identifiers at this layer so alias
/// methods can continue sharing one validation path.
fn handle_remote_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<Option<HostRpcResponse>> {
    let response = match method {
        "host.git.remotes" | "host.git.remote.list" => Some((
            serde_json::to_value(adapter.git.remotes(payload_cwd(payload))?)?,
            Vec::new(),
        )),
        "host.git.remoteExists" | "host.git.remote.exists" => {
            let remote = payload_required_str(payload, "remote", method)?;
            Some((
                json!({ "exists": adapter.git.remote_exists(payload_cwd(payload), &remote)? }),
                Vec::new(),
            ))
        }
        "host.git.remote.add" => {
            let name = payload_required_str(payload, "name", method)?;
            let url = payload_required_str(payload, "url", method)?;
            adapter.git.remote_add(payload_cwd(payload), &name, &url)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.remote.pull" => {
            let remote = payload_required_str(payload, "remote", method)?;
            let branch = payload_required_str(payload, "branch", method)?;
            adapter
                .git
                .remote_pull(payload_cwd(payload), &remote, &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.remote.push" => {
            let remote = payload_required_str(payload, "remote", method)?;
            let branch = payload_required_str(payload, "branch", method)?;
            adapter
                .git
                .remote_push(payload_cwd(payload), &remote, &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.remote.status" => {
            let remote = payload_required_str(payload, "remote", method)?;
            Some((
                json!({ "status": adapter.git.remote_status(payload_cwd(payload), &remote)? }),
                Vec::new(),
            ))
        }
        _ => None,
    };
    Ok(response)
}

/// Branch topology and history-rewriting operations.
///
/// This is the widest subdomain because it covers list/current/existence probes,
/// upstream metadata and merge/rebase/cherry-pick style mutations.
fn handle_branch_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<Option<HostRpcResponse>> {
    let response = match method {
        "host.git.listBranches" => {
            let cwd = payload_cwd(payload);
            let scope = payload
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("all");
            let value = match scope {
                "local" => json!({ "local": adapter.git.list_local_branches(cwd)? }),
                "remote" => json!({ "remote": adapter.git.list_remote_branches(cwd)? }),
                _ => {
                    let (local, remote) = adapter.git.list_all_branches(cwd)?;
                    json!({ "local": local, "remote": remote })
                }
            };
            Some((value, Vec::new()))
        }
        "host.git.branch.current" => Some((
            json!({ "branch": adapter.git.current_branch(Path::new(payload_cwd(payload)))? }),
            Vec::new(),
        )),
        "host.git.branch.listLocal" => Some((
            json!({ "branches": adapter.git.list_local_branches(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.branch.listRemote" => Some((
            json!({ "branches": adapter.git.list_remote_branches(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.branch.listAll" => {
            let (local, remote) = adapter.git.list_all_branches(payload_cwd(payload))?;
            Some((json!({ "local": local, "remote": remote }), Vec::new()))
        }
        "host.git.branchExists" | "host.git.branch.exists" => {
            let branch = payload_required_str(payload, "branch", method)?;
            Some((
                json!({ "exists": adapter.git.branch_exists(payload_cwd(payload), &branch)? }),
                Vec::new(),
            ))
        }
        "host.git.branch.existsLocal" => {
            let branch = payload_required_str(payload, "branch", method)?;
            Some((
                json!({ "exists": adapter.git.branch_exists_local(payload_cwd(payload), &branch)? }),
                Vec::new(),
            ))
        }
        "host.git.branch.existsRemote" => {
            let branch = payload_required_str(payload, "branch", method)?;
            Some((
                json!({ "exists": adapter.git.branch_exists_remote(payload_cwd(payload), &branch)? }),
                Vec::new(),
            ))
        }
        "host.git.branchCreate" | "host.git.branch.create" => {
            let branch = payload_required_str(payload, "branch", method)?;
            adapter.git.branch_create(payload_cwd(payload), &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branchSwitch" | "host.git.branch.switch" => {
            let branch = payload_required_str(payload, "branch", method)?;
            adapter.git.branch_switch(payload_cwd(payload), &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branchDelete" | "host.git.branch.delete" => {
            let branch = payload_required_str(payload, "branch", method)?;
            adapter.git.branch_delete(
                payload_cwd(payload),
                &branch,
                payload_bool(payload, "force", false),
            )?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.merge" => {
            let branch = payload_required_str(payload, "branch", method)?;
            adapter.git.merge(payload_cwd(payload), &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.mergeWithOptions" => {
            let branch = payload_required_str(payload, "branch", method)?;
            let options = GitMergeOptions {
                flags: payload_strings(payload, "flags"),
                strategy: payload_optional_str(payload, "strategy"),
                message: payload_optional_str(payload, "message"),
            };
            adapter
                .git
                .merge_with_options(payload_cwd(payload), &branch, options)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.mergeNoFF" => {
            let branch = payload_required_str(payload, "branch", method)?;
            adapter.git.merge_no_ff(payload_cwd(payload), &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.abortMerge" => {
            adapter.git.merge_abort(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.cherryPick" => {
            let commit = payload_required_str(payload, "commit", method)?;
            adapter.git.cherry_pick(payload_cwd(payload), &commit)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.continueCherryPick" => {
            adapter.git.cherry_pick_continue(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.abortCherryPick" => {
            adapter.git.cherry_pick_abort(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.rebase" => {
            let target_branch = payload_required_str(payload, "targetBranch", method)?;
            let options = GitRebaseOptions {
                interactive: payload_bool(payload, "interactive", false),
                onto: payload_optional_str(payload, "onto"),
                root: payload_bool(payload, "root", false),
            };
            adapter
                .git
                .rebase(payload_cwd(payload), &target_branch, options)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.abortRebase" => {
            adapter.git.rebase_abort(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.continueRebase" => {
            adapter.git.rebase_continue(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.skipRebase" => {
            adapter.git.rebase_skip(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.upstream" => Some((
            serde_json::to_value(adapter.git.upstream(payload_cwd(payload))?)?,
            Vec::new(),
        )),
        "host.git.branch.needsUpstream" => Some((
            json!({ "needsUpstream": adapter.git.needs_upstream(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.branch.setUpstream" => {
            let remote = payload_required_str(payload, "remote", method)?;
            let branch = payload_required_str(payload, "branch", method)?;
            adapter
                .git
                .set_upstream(payload_cwd(payload), &remote, &branch)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.branch.hasUnpushedCommits" => Some((
            json!({ "hasUnpushedCommits": adapter.git.has_unpushed_commits(payload_cwd(payload))? }),
            Vec::new(),
        )),
        _ => None,
    };
    Ok(response)
}

/// Staging plus commit/history/revert operations.
///
/// These methods all work against the working tree or commit graph and keep their
/// payload validation close to the history options they translate.
fn handle_stage_and_history_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<Option<HostRpcResponse>> {
    let response = match method {
        "host.git.stage.files" => Some((
            json!({ "files": adapter.git.stage_files(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.stage.add" => {
            adapter
                .git
                .add(payload_cwd(payload), &payload_strings(payload, "files"))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.stage.addAll" => {
            adapter.git.add_all(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.stage.reset" => {
            let file = payload_required_str(payload, "file", method)?;
            adapter.git.stage_reset(payload_cwd(payload), &file)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.stage.diff" => Some((
            json!({ "diff": adapter.git.stage_diff(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.commit" | "host.git.workspace.commit" => {
            let message = payload_required_str(payload, "message", method)?;
            adapter.git.commit(payload_cwd(payload), &message)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.workspace.commitAmend" => {
            let message = payload_optional_str(payload, "message");
            let no_edit = payload_bool(payload, "noEdit", false);
            adapter
                .git
                .commit_amend(payload_cwd(payload), message.as_deref(), no_edit)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.lastCommitMessage" | "host.git.workspace.lastCommitMessage" => Some((
            json!({ "message": adapter.git.last_commit_message(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.lastCommitHash" | "host.git.workspace.lastCommitHash" => Some((
            json!({ "hash": adapter.git.last_commit_hash(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.workspace.commitFiles" => {
            let commit = payload_required_str(payload, "commit", method)?;
            Some((
                json!({ "files": adapter.git.commit_files(payload_cwd(payload), &commit)? }),
                Vec::new(),
            ))
        }
        "host.git.commitLog" | "host.git.workspace.commitLog" => {
            let options = GitCommitLogOptions {
                limit: payload
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                author: payload_optional_str(payload, "author"),
                since: payload_optional_str(payload, "since"),
                until: payload_optional_str(payload, "until"),
                range: None,
                oneline: payload_bool(payload, "oneline", false),
                format: payload_optional_str(payload, "format"),
            };
            Some((
                serde_json::to_value(adapter.git.commit_log(payload_cwd(payload), options)?)?,
                Vec::new(),
            ))
        }
        "host.git.workspace.revert" => {
            let options = GitRevertOptions {
                no_commit: payload_bool(payload, "noCommit", false),
                mainline: payload
                    .get("mainline")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                no_edit: payload_bool(payload, "noEdit", false),
            };
            adapter.git.revert(
                payload_cwd(payload),
                &payload_strings(payload, "commits"),
                options,
            )?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.workspace.abortRevert" => {
            adapter.git.revert_abort(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.workspace.continueRevert" => {
            adapter.git.revert_continue(payload_cwd(payload))?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        _ => None,
    };
    Ok(response)
}

/// User metadata, tag operations and command-planning helpers.
///
/// These methods are comparatively isolated from the working tree and mainly
/// serialize small DTOs or normalized exec-command plans.
fn handle_identity_and_plan_domain(
    adapter: &HostRpcAdapter,
    method: &str,
    payload: &HostPayload,
) -> Result<Option<HostRpcResponse>> {
    let response = match method {
        "host.git.user.get" => Some((
            serde_json::to_value(adapter.git.user(payload_cwd(payload))?)?,
            Vec::new(),
        )),
        "host.git.user.set" => {
            let name = payload_required_str(payload, "name", method)?;
            let email = payload_required_str(payload, "email", method)?;
            adapter.git.set_user(payload_cwd(payload), &name, &email)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.tag.list" => Some((
            json!({ "tags": adapter.git.tags(payload_cwd(payload))? }),
            Vec::new(),
        )),
        "host.git.tag.create" => {
            let tag = payload_required_str(payload, "tag", method)?;
            let annotated = payload_bool(payload, "annotated", false);
            let message = payload_optional_str(payload, "message");
            if annotated || message.is_some() {
                adapter.git.tag_create_annotated(
                    payload_cwd(payload),
                    &tag,
                    message.as_deref().unwrap_or(&tag),
                )?;
            } else {
                adapter
                    .git
                    .tag_create_lightweight(payload_cwd(payload), &tag)?;
            }
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.tag.delete" => {
            let tag = payload_required_str(payload, "tag", method)?;
            adapter.git.tag_delete(payload_cwd(payload), &tag)?;
            Some((json!({ "ok": true }), Vec::new()))
        }
        "host.git.plan.addAll" => Some((
            command_to_json(adapter.git.command(adapter.git.plan_add_all())),
            Vec::new(),
        )),
        "host.git.plan.init" => Some((
            command_to_json(adapter.git.command(adapter.git.plan_init())),
            Vec::new(),
        )),
        "host.git.plan.commitMessage" => {
            let message = payload_required_str(payload, "message", method)?;
            Some((
                command_to_json(
                    adapter
                        .git
                        .command(adapter.git.plan_commit_message(&message)),
                ),
                Vec::new(),
            ))
        }
        "host.git.plan.commitAmend" => {
            let message = payload_optional_str(payload, "message");
            let no_edit = payload_bool(payload, "noEdit", false);
            Some((
                command_to_json(
                    adapter
                        .git
                        .command(adapter.git.plan_commit_amend(message.as_deref(), no_edit)),
                ),
                Vec::new(),
            ))
        }
        "host.git.plan.push" => {
            let remote = payload_required_str(payload, "remote", method)?;
            let branch = payload_required_str(payload, "branch", method)?;
            Some((
                command_to_json(adapter.git.command(adapter.git.plan_push(&remote, &branch))),
                Vec::new(),
            ))
        }
        "host.git.plan.pushTag" => {
            let remote = payload_required_str(payload, "remote", method)?;
            let tag = payload_required_str(payload, "tag", method)?;
            Some((
                command_to_json(
                    adapter
                        .git
                        .command(adapter.git.plan_push_tag(&remote, &tag)),
                ),
                Vec::new(),
            ))
        }
        "host.git.plan.tagCreateLightweight" => {
            let tag = payload_required_str(payload, "tag", method)?;
            Some((
                command_to_json(
                    adapter
                        .git
                        .command(adapter.git.plan_tag_create_lightweight(&tag)),
                ),
                Vec::new(),
            ))
        }
        "host.git.plan.tagCreateAnnotated" => {
            let tag = payload_required_str(payload, "tag", method)?;
            let message = payload_required_str(payload, "message", method)?;
            Some((
                command_to_json(
                    adapter
                        .git
                        .command(adapter.git.plan_tag_create_annotated(&tag, &message)),
                ),
                Vec::new(),
            ))
        }
        "host.git.plan.tagDelete" => {
            let tag = payload_required_str(payload, "tag", method)?;
            Some((
                command_to_json(adapter.git.command(adapter.git.plan_tag_delete(&tag))),
                Vec::new(),
            ))
        }
        "host.git.command" => Some((
            command_to_json(adapter.git.command(payload_strings(payload, "args"))),
            Vec::new(),
        )),
        _ => None,
    };
    Ok(response)
}
