//! 子进程执行抽象，统一 dry-run、命令记录与执行结果表达。
//!
//! 主要导出：new、shell、with_args、in_dir、with_env、dry_run。
//! 关键点：
//! - 包含异步/超时/取消等控制流
//! - 包含并发共享状态或消息通道
//! - 包含 unsafe 代码，修改前需确认安全假设
use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
use tokio_util::sync::CancellationToken;

type EventHandler = Arc<dyn Fn(ExecEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub use_shell: bool,
}

impl ExecCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            use_shell: false,
        }
    }

    pub fn shell(script: impl Into<String>) -> Self {
        Self {
            program: script.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            use_shell: true,
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn in_dir(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    fn validate(&self) -> Result<()> {
        if self.program.trim().is_empty() {
            anyhow::bail!("exec command program must not be empty");
        }
        if self.use_shell && !self.args.is_empty() {
            anyhow::bail!(
                "shell commands must be created via ExecCommand::shell without argv args"
            );
        }
        Ok(())
    }

    fn display_name(&self) -> String {
        if self.use_shell {
            format!("shell: {}", self.program)
        } else if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecEvent {
    Started {
        command: String,
        cwd: Option<String>,
    },
    Stdout(String),
    Stderr(String),
    TimedOut {
        timeout_ms: u64,
    },
    Cancelled,
    Finished {
        exit_code: i32,
    },
}

#[derive(Clone, Default)]
pub struct ExecRunOptions {
    pub timeout: Option<Duration>,
    pub cancellation: Option<CancellationToken>,
    pub on_event: Option<EventHandler>,
    pub kill_process_tree: bool,
}

impl std::fmt::Debug for ExecRunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecRunOptions")
            .field("timeout", &self.timeout)
            .field("has_cancellation", &self.cancellation.is_some())
            .field("has_event_handler", &self.on_event.is_some())
            .field("kill_process_tree", &self.kill_process_tree)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub skipped: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecErrorCode {
    BinaryMissing,
    PermissionDenied,
    SpawnFailed,
    CommandFailed,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecError {
    pub code: ExecErrorCode,
    pub message: String,
    pub command: ExecCommand,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ExecError {
    fn code_as_str(&self) -> &'static str {
        match self.code {
            ExecErrorCode::BinaryMissing => "binary_missing",
            ExecErrorCode::PermissionDenied => "permission_denied",
            ExecErrorCode::SpawnFailed => "spawn_failed",
            ExecErrorCode::CommandFailed => "command_failed",
            ExecErrorCode::TimedOut => "timed_out",
            ExecErrorCode::Cancelled => "cancelled",
        }
    }

    fn from_spawn_failure(command: ExecCommand, error: std::io::Error) -> Self {
        let code = classify_io_error(&error);
        Self {
            code,
            message: format!("failed to spawn {}: {error}", command.display_name()),
            command,
            exit_code: error.raw_os_error(),
            stdout: String::new(),
            stderr: error.to_string(),
        }
    }

    fn from_result(command: ExecCommand, result: ExecResult) -> Self {
        let code = if result.timed_out {
            ExecErrorCode::TimedOut
        } else if result.cancelled {
            ExecErrorCode::Cancelled
        } else {
            ExecErrorCode::CommandFailed
        };
        let message = if result.timed_out {
            format!("command {} timed out", command.display_name())
        } else if result.cancelled {
            format!("command {} was cancelled", command.display_name())
        } else {
            format_command_failure_message(
                &command,
                result.exit_code,
                &result.stdout,
                &result.stderr,
            )
        };
        Self {
            code,
            message,
            command,
            exit_code: Some(result.exit_code),
            stdout: result.stdout,
            stderr: result.stderr,
        }
    }
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code_as_str(), self.message)
    }
}

impl std::error::Error for ExecError {}

#[derive(Debug, Clone)]
pub struct ExecService {
    dry_run: bool,
    // 记录执行历史，用于：
    // - workflow 最终输出 command_plans
    // - 调试时回放“这次到底跑了哪些子进程”
    //
    // 为什么是 `Arc<Mutex<Vec<ExecCommand>>>`？
    // - ExecService 会被 clone 到多个 workflow/service 中共享；
    // - 每次 run 都会往历史里 push 一条记录；
    // - 因此需要共享所有权（Arc）+ 并发可变保护（Mutex）。
    history: Arc<Mutex<Vec<ExecCommand>>>,
}

impl ExecService {
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn dry_run() -> Self {
        Self::new(true)
    }

    pub fn working_dir(&self) -> Result<String> {
        std::env::current_dir()
            .map(|cwd| cwd.display().to_string())
            .context("failed to resolve current working directory")
    }

    pub fn run(&self, command: ExecCommand) -> Result<ExecResult> {
        self.run_with_options(command, ExecRunOptions::default())
    }

    pub fn run_with_options(
        &self,
        command: ExecCommand,
        options: ExecRunOptions,
    ) -> Result<ExecResult> {
        let service = self.clone();
        self.block_on_result(async move { service.run_with_options_async(command, options).await })
    }

    pub async fn run_async(&self, command: ExecCommand) -> Result<ExecResult> {
        self.run_with_options_async(command, ExecRunOptions::default())
            .await
    }

    pub async fn run_with_options_async(
        &self,
        command: ExecCommand,
        options: ExecRunOptions,
    ) -> Result<ExecResult> {
        // 历史记录在最前面 push，这样即使后面 spawn 失败，
        // 上层回看 command_plans 时也能知道“本来尝试执行过什么”。
        self.history
            .lock()
            .expect("exec history poisoned")
            .push(command.clone());
        command.validate()?;

        if self.dry_run {
            self.emit_event(
                &options,
                ExecEvent::Started {
                    command: command.display_name(),
                    cwd: command.cwd.clone(),
                },
            );
            self.emit_event(&options, ExecEvent::Finished { exit_code: 0 });
            return Ok(ExecResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                skipped: true,
                timed_out: false,
                cancelled: false,
            });
        }

        let mut process = self.build_process(&command, options.kill_process_tree)?;
        let mut child = process
            .spawn()
            .map_err(|error| ExecError::from_spawn_failure(command.clone(), error))?;
        let pid = child.id();

        self.emit_event(
            &options,
            ExecEvent::Started {
                command: command.display_name(),
                cwd: command.cwd.clone(),
            },
        );

        let stdout_reader = child
            .stdout
            .take()
            .map(|stdout| spawn_reader(BufReader::new(stdout), true, options.on_event.clone()));
        let stderr_reader = child
            .stderr
            .take()
            .map(|stderr| spawn_reader(BufReader::new(stderr), false, options.on_event.clone()));

        // 注意这里的等待顺序：
        // 1) 先等子进程生命周期结束（正常退出 / timeout / cancel）
        // 2) 再 `await` stdout/stderr reader 把尾部输出收完
        //
        // 如果顺序反过来，reader 可能会一直等到管道 EOF，而 EOF 又依赖子进程先退出，
        // 控制流会更绕，也更容易在 timeout/cancel 场景下漏掉最后几行输出。
        let cancellation = options.cancellation.clone().unwrap_or_default();
        let termination = wait_for_termination(
            &mut child,
            pid,
            options.timeout,
            cancellation.clone(),
            options.kill_process_tree,
        )
        .await?;

        let stdout_chunks = match stdout_reader {
            Some(reader) => reader.await.map_err(join_error)?,
            None => Ok(Vec::new()),
        }?;
        let stderr_chunks = match stderr_reader {
            Some(reader) => reader.await.map_err(join_error)?,
            None => Ok(Vec::new()),
        }?;

        match termination {
            ExecTermination::Exited(exit_code) => {
                self.emit_event(&options, ExecEvent::Finished { exit_code });
                Ok(ExecResult {
                    exit_code,
                    stdout: stdout_chunks.join("\n"),
                    stderr: stderr_chunks.join("\n"),
                    skipped: false,
                    timed_out: false,
                    cancelled: false,
                })
            }
            ExecTermination::TimedOut(timeout) => {
                self.emit_event(
                    &options,
                    ExecEvent::TimedOut {
                        timeout_ms: timeout.as_millis() as u64,
                    },
                );
                Ok(ExecResult {
                    exit_code: -1,
                    stdout: stdout_chunks.join("\n"),
                    stderr: stderr_chunks.join("\n"),
                    skipped: false,
                    timed_out: true,
                    cancelled: false,
                })
            }
            ExecTermination::Cancelled => {
                self.emit_event(&options, ExecEvent::Cancelled);
                Ok(ExecResult {
                    exit_code: -2,
                    stdout: stdout_chunks.join("\n"),
                    stderr: stderr_chunks.join("\n"),
                    skipped: false,
                    timed_out: false,
                    cancelled: true,
                })
            }
        }
    }

    pub fn run_checked(&self, command: ExecCommand) -> Result<ExecResult> {
        self.run_checked_with_options(command, ExecRunOptions::default())
    }

    pub fn run_checked_with_options(
        &self,
        command: ExecCommand,
        options: ExecRunOptions,
    ) -> Result<ExecResult> {
        // `run_with_options` 只负责“执行并返回结果”，哪怕 exit_code != 0 也不会自动报错。
        // `run_checked_*` 则额外加上一层“非 0 退出码视为错误”的语义，
        // 方便上层在需要严格失败时少写一层判断。
        let result = self.run_with_options(command.clone(), options)?;
        if result.skipped || result.exit_code == 0 {
            return Ok(result);
        }

        Err(ExecError::from_result(command, result).into())
    }

    pub async fn run_checked_async(&self, command: ExecCommand) -> Result<ExecResult> {
        let result = self.run_async(command.clone()).await?;
        if result.skipped || result.exit_code == 0 {
            return Ok(result);
        }

        Err(ExecError::from_result(command, result).into())
    }

    pub fn history(&self) -> Vec<ExecCommand> {
        self.history.lock().expect("exec history poisoned").clone()
    }

    fn emit_event(&self, options: &ExecRunOptions, event: ExecEvent) {
        if let Some(handler) = options.on_event.as_ref() {
            handler(event);
        }
    }

    fn build_process(&self, command: &ExecCommand, kill_process_tree: bool) -> Result<Command> {
        // `ExecCommand` 是平台无关的抽象，这里才真正把它翻译成 `tokio::process::Command`。
        // - `use_shell=false`：直接执行二进制 + argv
        // - `use_shell=true`：交给 shell 解释脚本字符串
        let mut process = if command.use_shell {
            shell_process(&command.program)
        } else {
            let mut process = Command::new(&command.program);
            process.args(&command.args);
            process
        };
        if let Some(cwd) = &command.cwd {
            process.current_dir(PathBuf::from(cwd));
        }
        process.envs(&command.env);
        process.stdin(Stdio::null());
        process.stdout(Stdio::piped());
        process.stderr(Stdio::piped());
        // `kill_on_drop(true)` 是一个很重要的兜底：
        // 如果 future 被取消、panic，或者上层提前丢弃了 Child，
        // Tokio 会尽量在 drop 时杀掉子进程，减少孤儿进程残留。
        process.kill_on_drop(true);
        configure_process_group(&mut process, kill_process_tree)?;
        Ok(process)
    }

    fn block_on_result<T: Send + 'static>(
        &self,
        future: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        // 这个函数解决的是“同步 API 如何安全等待 async 实现”的问题。
        //
        // 两种情况：
        // 1. 当前线程已经在 Tokio runtime 里：
        //    不能直接再 `block_on`，否则容易造成嵌套 runtime 问题；
        //    这里选择另起一个线程，在线程里跑一个 current-thread runtime。
        // 2. 当前不在 runtime 里：
        //    直接临时创建一个 current-thread runtime 即可。
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| anyhow!("failed to create exec runtime: {error}"))?
                    .block_on(future)
            })
            .join()
            .map_err(|_| anyhow!("exec runtime thread panicked"))?
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| anyhow!("failed to create exec runtime: {error}"))?
                .block_on(future)
        }
    }
}

impl Default for ExecService {
    fn default() -> Self {
        Self::new(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecTermination {
    Exited(i32),
    TimedOut(Duration),
    Cancelled,
}

fn spawn_reader<R>(
    reader: BufReader<R>,
    is_stdout: bool,
    on_event: Option<EventHandler>,
) -> tokio::task::JoinHandle<Result<Vec<String>>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        // stdout/stderr 各自起一个 reader task：
        // - 一边读，一边把行通过事件回调抛给上层
        // - 同时把完整输出收集到 Vec，供最终 ExecResult 返回
        //
        // 这样既能做实时 UI，又不会丢最终完整日志。
        let mut reader = reader;
        let mut line = String::new();
        let mut chunks = Vec::new();
        loop {
            line.clear();
            let read = reader.read_line(&mut line).await?;
            if read == 0 {
                break;
            }
            let chunk = line.trim_end_matches(['\n', '\r']).to_string();
            if let Some(handler) = on_event.as_ref() {
                handler(if is_stdout {
                    ExecEvent::Stdout(chunk.clone())
                } else {
                    ExecEvent::Stderr(chunk.clone())
                });
            }
            chunks.push(chunk);
        }
        Ok(chunks)
    })
}

async fn wait_for_termination(
    child: &mut Child,
    pid: Option<u32>,
    timeout: Option<Duration>,
    cancellation: CancellationToken,
    kill_process_tree: bool,
) -> Result<ExecTermination> {
    // 这里的 `select!` 是 exec 控制流的核心：
    // 它在“正常退出 / 超时 / 外部取消”三种终止条件之间竞争。
    let termination = if let Some(timeout) = timeout {
        tokio::select! {
            status = child.wait() => ExecTermination::Exited(status?.code().unwrap_or_default()),
            _ = tokio::time::sleep(timeout) => {
                // 超时分支和取消分支都主动终止子进程，而不是只返回一个状态。
                // 否则调用方虽然知道“超时了”，但实际子进程还会继续在后台跑。
                terminate_child(child, pid, kill_process_tree).await?;
                ExecTermination::TimedOut(timeout)
            }
            _ = cancellation.cancelled() => {
                terminate_child(child, pid, kill_process_tree).await?;
                ExecTermination::Cancelled
            }
        }
    } else {
        tokio::select! {
            status = child.wait() => ExecTermination::Exited(status?.code().unwrap_or_default()),
            _ = cancellation.cancelled() => {
                terminate_child(child, pid, kill_process_tree).await?;
                ExecTermination::Cancelled
            }
        }
    };
    if !matches!(termination, ExecTermination::Exited(_)) {
        // 如果是 timeout/cancel，前面只发出了终止信号；
        // 这里再 `wait()` 一次，尽量把子进程真正收尸，避免 zombie process。
        let _ = child.wait().await;
    }
    Ok(termination)
}

async fn terminate_child(
    child: &mut Child,
    pid: Option<u32>,
    kill_process_tree: bool,
) -> Result<()> {
    #[cfg(unix)]
    if kill_process_tree {
        if let Some(pid) = pid {
            // `killpg(pid, SIGKILL)` 的意思是“杀掉整个进程组”，而不只是主进程自己。
            // 这对 shell 脚本、构建工具链很重要，因为它们往往会再 fork 出子进程。
            //
            // 这里依赖前面的 `configure_process_group()`：
            // - 子进程启动时被放进自己的进程组
            // - 所以现在才能按“组”发送信号，而不是误伤当前 Rust 进程所在组
            let result = unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("failed to terminate process group");
                }
            }
        }
    }

    // 即使没有 kill 进程组，也至少要把主子进程 kill 掉。
    let _ = child.start_kill();
    Ok(())
}

fn join_error(error: tokio::task::JoinError) -> anyhow::Error {
    anyhow!("exec stream task failed: {error}")
}

fn classify_io_error(error: &std::io::Error) -> ExecErrorCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => ExecErrorCode::BinaryMissing,
        std::io::ErrorKind::PermissionDenied => ExecErrorCode::PermissionDenied,
        _ => ExecErrorCode::SpawnFailed,
    }
}

fn format_command_failure_message(
    command: &ExecCommand,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> String {
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        ""
    };
    if detail.is_empty() {
        format!(
            "command {} exited with code {}",
            command.display_name(),
            exit_code
        )
    } else {
        format!(
            "command {} exited with code {}: {}",
            command.display_name(),
            exit_code,
            detail
        )
    }
}

fn shell_process(script: &str) -> Command {
    #[cfg(unix)]
    {
        // `/bin/sh -lc` 让 shell 字符串按用户熟悉的 shell 语义执行：
        // 支持管道、重定向、变量展开等。
        let mut process = Command::new("/bin/sh");
        process.arg("-lc").arg(script);
        process
    }
    #[cfg(windows)]
    {
        let mut process = Command::new("cmd");
        process.arg("/C").arg(script);
        process
    }
}

fn configure_process_group(process: &mut Command, kill_process_tree: bool) -> Result<()> {
    #[cfg(unix)]
    {
        if kill_process_tree {
            // Put the child into its own process group so cancellation can reap descendants.
            // `pre_exec` 在 `fork` 后、`exec` 前执行：
            // 这是 Unix 上调整子进程组/会话属性的经典位置。
            unsafe {
                process.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
    }

    let _ = kill_process_tree;
    Ok(())
}

#[cfg(test)]
mod tests;
