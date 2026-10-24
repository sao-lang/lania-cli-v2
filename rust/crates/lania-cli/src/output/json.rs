//! JSON 输出准备：把 `lania_host::CommandExecution` 转成统一输出 envelope。
//!
//! 统一 envelope 只携带：
//! - 命令名与参数（`command`）
//! - 工作目录（`cwd`）
//! - 退出码（`exitCode`）
//! - 业务结果（`result`）
//! - 可选错误（`error`）与 git 分支（`git`）
//!
//! 这里只做“结构调整”和文案本地化，不做最终字符串渲染。

use lania_command::CommandContext;
use serde_json::{json, Value};

use crate::profile::localized_user_message;
use crate::OutputProfile;

fn localize_json_strings(value: &mut Value, locale: &str) {
    match value {
        // 约定：需要本地化的用户可见文案会以 string 形式出现，直接替换即可。
        Value::String(text) => {
            *text = localized_user_message(locale, text);
        }
        Value::Array(items) => {
            for item in items {
                localize_json_strings(item, locale);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                localize_json_strings(value, locale);
            }
        }
        _ => {}
    }
}

fn command_value(context: &CommandContext) -> Value {
    // 位置参数在解析结果里同时出现在 args/options，输出时只保留在 args。
    let mut options = context.argv.options.clone();
    for key in context.argv.args.keys() {
        options.remove(key);
    }
    json!({
        "name": context.name,
        "args": context.argv.args,
        "options": options,
    })
}

fn execution_context(execution: &lania_host::CommandExecution) -> &CommandContext {
    match execution {
        lania_host::CommandExecution::Bridge { context, .. }
        | lania_host::CommandExecution::Workflow { context, .. }
        | lania_host::CommandExecution::TemplateInfo { context, .. } => context,
    }
}

fn print_payload(execution: &lania_host::CommandExecution) -> Value {
    match execution {
        lania_host::CommandExecution::Bridge { exchange, .. } => exchange
            .response
            .result
            .clone()
            .unwrap_or(serde_json::Value::Null),
        lania_host::CommandExecution::Workflow { execution, .. } => {
            let mut payload =
                serde_json::to_value(execution).expect("workflow execution serializes");
            // bridge_steps 是执行期调试信息，不属于必要结果。
            if let Some(object) = payload.as_object_mut() {
                object.remove("bridge_steps");
            }
            payload
        }
        lania_host::CommandExecution::TemplateInfo { output, .. } => {
            // 模板查询本身不写 `kind`，但旧输出依赖 `kind: template_info` 做渲染分派。
            // 这里补回该标记，保持 `result` 的结构与历史输出一致。
            let mut payload = output.clone();
            if let Some(object) = payload.as_object_mut() {
                object
                    .entry("kind")
                    .or_insert_with(|| json!("template_info"));
            }
            payload
        }
    }
}

fn bridge_error_message(execution: &lania_host::CommandExecution) -> Option<String> {
    match execution {
        lania_host::CommandExecution::Bridge { exchange, .. } => exchange
            .response
            .error
            .as_ref()
            .map(|error| error.message.clone()),
        _ => None,
    }
}

fn strip_internal_flags(payload: &mut Value) {
    if let Some(object) = payload.as_object_mut() {
        object.remove("_interactiveRendered");
    }
}

pub(crate) fn execution_json_value(
    execution: &lania_host::CommandExecution,
    profile: &OutputProfile,
    cwd: &str,
    git_branch: Option<&str>,
) -> Value {
    let context = execution_context(execution);
    let mut payload = print_payload(execution);
    strip_internal_flags(&mut payload);

    let mut value = json!({
        "kind": "result",
        "command": command_value(context),
        "cwd": cwd,
        "exitCode": execution.exit_code(),
        "result": payload,
    });

    if let Some(message) = bridge_error_message(execution) {
        value["error"] = json!({ "message": message });
    }
    if let Some(branch) = git_branch {
        value["git"] = json!({ "branch": branch });
    }

    localize_json_strings(&mut value, profile.locale.as_str());
    value
}

pub(crate) fn error_json_value(
    command: Option<&CommandContext>,
    cwd: &str,
    exit_code: i32,
    message: &str,
) -> Value {
    let mut value = json!({
        "kind": "error",
        "cwd": cwd,
        "exitCode": exit_code,
        "error": { "message": message },
    });
    if let Some(context) = command {
        value["command"] = command_value(context);
    }
    value
}
