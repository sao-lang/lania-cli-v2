//! 极简 HTTP 客户端：只覆盖 Provider 需要的 GET（文本 / JSON / 大文件流）。
//!
//! 用阻塞式 `ureq` 而不是异步栈，原因有三：
//! - `lan env` 是**一次性的短命令**，没有并发请求需求；
//! - TUI 的动作回调本来就是同步的，异步客户端反而要把结果搬回同步上下文；
//! - 少一层 tokio runtime，二进制体积与启动时间都更可控。
//!
//! 环境变量：
//! - `RUNEX_OFFLINE=1` 完全禁用网络（清单/缓存命中仍可用）
//! - `RUNEX_HTTP_TIMEOUT_SECS` 连接/单次读取超时（默认 30）
//! - `RUNEX_DOWNLOAD_TIMEOUT_SECS` 单个产物下载的总时限（默认 1800，覆盖慢速网络）
//! - `GITHUB_TOKEN` / `GH_TOKEN` 可选：GitHub API 匿名配额只有 60 次/小时，
//!   提供 token 后提升到 5000 次/小时（Python/Deno/Bun 的版本清单走 GitHub API）
//! - 标准 `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` 由 ureq 自动读取
//!
//! 注意这里**不使用** `timeout_global`：运行时的产物动辄上百 MB，
//! 「整个请求必须在 30 秒内完成」会直接导致正常下载被判定为超时。
//! 因此只限制连接与两次读取之间的间隔，总时长由下载专用的上限兜底。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ureq::Agent;

use crate::{NetworkError, RuntimeError};

pub const OFFLINE_ENV: &str = "RUNEX_OFFLINE";
pub const TIMEOUT_ENV: &str = "RUNEX_HTTP_TIMEOUT_SECS";
pub const DOWNLOAD_TIMEOUT_ENV: &str = "RUNEX_DOWNLOAD_TIMEOUT_SECS";
const USER_AGENT: &str = concat!("lania-cli/", env!("CARGO_PKG_VERSION"));

/// 下载进度回调：`(已下载字节, Content-Length)`。
///
/// `Content-Length` 缺失时第二个参数为 `None`。
pub type DownloadProgressFn<'a> = &'a mut dyn FnMut(u64, Option<u64>);

#[derive(Debug, Clone)]
pub struct HttpClient {
    /// 元数据请求（版本清单 / 校验和）：body 很小时也要求快速失败。
    agent: Agent,
    /// 产物下载：body 动辄上百 MB，总时限必须用 `RUNEX_DOWNLOAD_TIMEOUT_SECS`。
    download_agent: Agent,
    offline: bool,
    download_timeout: Duration,
    github_token: Option<String>,
}

impl HttpClient {
    pub fn new() -> Self {
        let timeout = Duration::from_secs(env_secs(TIMEOUT_ENV, 30));
        let download_timeout = Duration::from_secs(env_secs(DOWNLOAD_TIMEOUT_ENV, 1800));
        // ureq 3 的 `timeout_recv_body` 是**整个响应体**的预算（不是每次读），
        // 若它与 30s 连接超时共用，任何超过 30 秒的产物下载都会被判超时——
        // 慢网下 100MB+ 的 JDK / Rust 包必然失败。所以下载单独用一个 agent。
        let agent = build_agent(timeout, timeout);
        let download_agent = build_agent(timeout, download_timeout);
        let offline = std::env::var(OFFLINE_ENV)
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                !matches!(value.as_str(), "" | "0" | "false" | "no")
            })
            .unwrap_or(false);
        let github_token = ["GITHUB_TOKEN", "GH_TOKEN"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Self {
            agent,
            download_agent,
            offline,
            download_timeout,
            github_token,
        }
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    /// 是否配置了 GitHub token（决定 API 配额是 5000 还是 60 次/小时）。
    pub fn has_github_token(&self) -> bool {
        self.github_token.is_some()
    }

    /// 单个产物下载的总时限（供上层判断是否需要提示用户）。
    pub fn download_timeout(&self) -> Duration {
        self.download_timeout
    }

    fn ensure_online(&self, url: &str) -> Result<(), RuntimeError> {
        if self.offline {
            return Err(NetworkError::Request {
                url: url.to_string(),
                detail: format!("offline mode is enabled ({OFFLINE_ENV}=1)"),
            }
            .into());
        }
        Ok(())
    }

    /// GET 文本内容（用于 checksum / 简单清单）。
    pub fn text(&self, url: &str) -> Result<String, RuntimeError> {
        self.ensure_online(url)?;
        let mut response = self.get(url)?;
        let mut body = String::new();
        response
            .read_to_string(&mut body)
            .map_err(|error| NetworkError::Request {
                url: url.to_string(),
                detail: error.to_string(),
            })?;
        Ok(body)
    }

    /// GET 并解析 JSON。
    pub fn json(&self, url: &str) -> Result<serde_json::Value, RuntimeError> {
        let body = self.text(url)?;
        serde_json::from_str(&body).map_err(|error| RuntimeError::Upstream {
            url: url.to_string(),
            detail: format!("invalid JSON: {error}"),
        })
    }

    /// 下载到 `target`（流式写盘，避免把几百 MB 的产物读进内存）。
    ///
    /// 必须校验实际字节数与 `Content-Length` 一致：连接中途断开时
    /// 读取循环会“正常”返回已写入的字节数，如果不比对长度，
    /// 会得到一个能被缓存、但解压必然失败的半截产物。
    pub fn download(&self, url: &str, target: &Path) -> Result<u64, RuntimeError> {
        self.download_with_progress(url, target, None)
    }

    /// 与 [`Self::download`] 相同，但每约 150ms 回调一次进度。
    ///
    /// 下载先写到 `<target>.part`：中断后下次会用 HTTP `Range` 续传；
    /// 服务端不支持 Range（返回 200）时自动从头开始。
    pub fn download_with_progress(
        &self,
        url: &str,
        target: &Path,
        mut on_progress: Option<DownloadProgressFn<'_>>,
    ) -> Result<u64, RuntimeError> {
        self.ensure_online(url)?;
        let partial = partial_path_for(target);
        if let Some(parent) = partial.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existing = std::fs::metadata(&partial)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let range = (existing > 0).then_some(existing);
        let response = self.get_response_with(&self.download_agent, url, range)?;
        let status = response.status().as_u16();
        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());

        let (offset, expected) = if status == 206 && existing > 0 {
            (
                existing,
                content_length.map(|remaining| existing + remaining),
            )
        } else {
            // 服务端忽略了 Range：从头开始，避免把新内容追加到旧 .part 后面。
            (0, content_length)
        };

        let mut file = if offset > 0 {
            std::fs::OpenOptions::new().append(true).open(&partial)?
        } else {
            std::fs::File::create(&partial)?
        };
        let mut reader = response.into_body().into_reader();
        let mut buffer = [0u8; 64 * 1024];
        let mut written = offset;
        let mut last_report = Instant::now();
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| NetworkError::Request {
                    url: url.to_string(),
                    detail: error.to_string(),
                })?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|error| NetworkError::Request {
                    url: url.to_string(),
                    detail: error.to_string(),
                })?;
            written += read as u64;
            if let Some(callback) = on_progress.as_mut() {
                let now = Instant::now();
                if now.duration_since(last_report) >= Duration::from_millis(150) {
                    callback(written, expected);
                    last_report = now;
                }
            }
        }
        if let Some(callback) = on_progress.as_mut() {
            callback(written, expected);
        }
        file.flush()?;
        drop(file);

        if let Some(expected) = expected {
            if written != expected {
                // 保留 .part，下一次继续续传；半截文件不会进入正式缓存。
                return Err(NetworkError::Request {
                    url: url.to_string(),
                    detail: format!("truncated download: wrote {written} of {expected} bytes"),
                }
                .into());
            }
        }
        let _ = std::fs::remove_file(target);
        std::fs::rename(&partial, target)?;
        Ok(written)
    }

    /// 下载到内存（只用于小的校验和/清单文件）。
    pub fn bytes(&self, url: &str) -> Result<Vec<u8>, RuntimeError> {
        self.ensure_online(url)?;
        let mut response = self.get(url)?;
        let mut buffer = Vec::new();
        response
            .read_to_end(&mut buffer)
            .map_err(|error| NetworkError::Request {
                url: url.to_string(),
                detail: error.to_string(),
            })?;
        Ok(buffer)
    }

    fn get(&self, url: &str) -> Result<Box<dyn Read + Send + 'static>, RuntimeError> {
        let response = self.get_response(url)?;
        Ok(Box::new(response.into_body().into_reader()) as Box<dyn Read + Send>)
    }

    /// 发起 GET 并返回完整响应（需要 `Content-Length` 等头部时使用）。
    fn get_response(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, RuntimeError> {
        self.get_response_with(&self.agent, url, None)
    }

    fn get_response_with(
        &self,
        agent: &Agent,
        url: &str,
        range: Option<u64>,
    ) -> Result<ureq::http::Response<ureq::Body>, RuntimeError> {
        // 上游偶发 5xx（GitHub 的 504 很常见）不该让整个 install 失败：
        // 只对「可重试」错误退避重试，4xx 立即返回（路径/权限问题重试也没用）。
        const ATTEMPTS: usize = 3;
        let is_github_api = url.contains("api.github.com");
        let request_url = rewrite_url(url);
        let range_header = range.map(|offset| format!("bytes={offset}-"));
        let mut last: Option<RuntimeError> = None;
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * attempt as u64));
            }
            let mut request = agent.get(&request_url);
            if let Some(value) = range_header.as_deref() {
                request = request.header("Range", value);
            }
            if is_github_api {
                // GitHub 要求 API 请求带 `Accept`，否则会走旧格式或直接 415。
                request = request
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28");
                if let Some(token) = self.github_token.as_deref() {
                    request = request.header("Authorization", &format!("Bearer {token}"));
                }
            }
            match request.call() {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let retryable = match &error {
                        ureq::Error::StatusCode(status) => *status >= 500,
                        _ => true,
                    };
                    let rendered = match &error {
                        ureq::Error::StatusCode(403) if is_github_api => {
                            "HTTP 403 — GitHub API rate limit reached (60 requests/hour without a \
                             token); set GITHUB_TOKEN to raise the limit to 5000"
                                .to_string()
                        }
                        ureq::Error::StatusCode(status) => format!("HTTP {status}"),
                        other => other.to_string(),
                    };
                    last = Some(match error {
                        ureq::Error::StatusCode(status) => NetworkError::Status {
                            url: url.to_string(),
                            status,
                        }
                        .into(),
                        _ => NetworkError::Request {
                            url: url.to_string(),
                            detail: rendered,
                        }
                        .into(),
                    });
                    if !retryable {
                        break;
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| {
            NetworkError::Request {
                url: url.to_string(),
                detail: "request failed after retries".to_string(),
            }
            .into()
        }))
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 可选的 GitHub / Node 镜像改写。
///
/// - `RUNEX_GITHUB_MIRROR` 替换 `https://github.com`
/// - `RUNEX_GITHUB_API_MIRROR` 替换 `https://api.github.com`
/// - `RUNEX_NODE_MIRROR` 在 Node Provider 内替换 `https://nodejs.org/dist`
///
/// 只改请求 URL；缓存键、进度显示仍使用原始 URL，避免镜像切换后缓存失效。
fn rewrite_url(url: &str) -> String {
    for (key, prefix) in [
        ("RUNEX_GITHUB_API_MIRROR", "https://api.github.com"),
        ("RUNEX_GITHUB_MIRROR", "https://github.com"),
    ] {
        let Ok(mirror) = std::env::var(key) else {
            continue;
        };
        let mirror = mirror.trim().trim_end_matches('/');
        if mirror.is_empty() {
            continue;
        }
        if let Some(rest) = url.strip_prefix(prefix) {
            return format!("{mirror}{rest}");
        }
    }
    url.to_string()
}

/// 下载缓存路径：`<downloads>/<sha of url>.<file name>`，保证同名不同源的产物不互相覆盖。
pub fn cache_path_for(downloads_dir: &Path, url: &str) -> PathBuf {
    let file_name = url
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("artifact");
    let digest = crate::checksum::hex_digest_sha256(url.as_bytes());
    downloads_dir.join(format!("{}-{file_name}", &digest[..16]))
}

/// 下载中的临时文件：`<target>.part`，用于中断后续传。
pub fn partial_path_for(target: &Path) -> PathBuf {
    let mut value = target.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

fn env_secs(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// 构造一个 agent。
///
/// - `connect`：连接 / 响应头 / 单次读写的上限（老版本 `timeout` 语义）；
/// - `body`：**整个响应体**的预算。元数据用 `connect` 即可快速失败，
///   下载则必须放宽到 `RUNEX_DOWNLOAD_TIMEOUT_SECS`。
fn build_agent(connect: Duration, body: Duration) -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(connect))
        .timeout_recv_response(Some(connect))
        // 每次读重新计时：真正防的是「连接卡死」而不是「整体慢」。
        .timeout_per_read(Some(connect))
        .timeout_recv_body(Some(body))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn cache_path_is_stable_and_collision_free() {
        let dir = Path::new("/tmp/downloads");
        let a = cache_path_for(dir, "https://nodejs.org/dist/v20.11.1/node-v20.11.1.tar.gz");
        let b = cache_path_for(dir, "https://nodejs.org/dist/v20.11.1/node-v20.11.1.tar.gz");
        let c = cache_path_for(dir, "https://mirror.example/v20.11.1/node-v20.11.1.tar.gz");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.to_string_lossy().ends_with("node-v20.11.1.tar.gz"));
    }

    #[test]
    fn download_resumes_from_partial_file() {
        let body: Vec<u8> = (0..100_000).map(|index| (index % 251) as u8).collect();
        let split = 40_000;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server_body = body.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0u8; 2048];
            let read = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
            assert!(request.contains("range: bytes=40000-"), "{request}");
            let remaining = server_body.len() - split;
            write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {remaining}\r\nContent-Range: bytes 40000-99999/100000\r\nConnection: close\r\n\r\n"
            )
            .expect("write headers");
            stream
                .write_all(&server_body[split..])
                .expect("write remaining body");
        });

        let dir = std::env::temp_dir().join(format!("runex-http-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let target = dir.join("artifact.bin");
        std::fs::write(partial_path_for(&target), &body[..split]).expect("seed partial");
        let written = HttpClient::new()
            .download(&format!("http://{address}/artifact.bin"), &target)
            .expect("download succeeds");
        server.join().expect("test server joins");

        assert_eq!(written, body.len() as u64);
        assert_eq!(std::fs::read(&target).expect("read target"), body);
        assert!(!partial_path_for(&target).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_reports_progress_and_writes_the_body() {
        let body = vec![b'x'; 200_000];
        let expected = body.len();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server_body = body.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {expected}\r\nConnection: close\r\n\r\n"
            )
            .expect("write headers");
            stream.write_all(&server_body).expect("write body");
        });

        let dir = std::env::temp_dir().join(format!("runex-http-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let target = dir.join("artifact.bin");
        let mut events = Vec::new();
        let mut report = |downloaded, total| events.push((downloaded, total));
        let written = HttpClient::new()
            .download_with_progress(
                &format!("http://{address}/artifact.bin"),
                &target,
                Some(&mut report),
            )
            .expect("download succeeds");
        server.join().expect("test server joins");

        assert_eq!(written, expected as u64);
        assert_eq!(std::fs::read(&target).expect("read target"), body);
        assert!(!events.is_empty(), "progress callback must fire");
        assert_eq!(events.last().unwrap().0, expected as u64);
        assert_eq!(events.last().unwrap().1, Some(expected as u64));
        let _ = std::fs::remove_dir_all(dir);
    }
}
