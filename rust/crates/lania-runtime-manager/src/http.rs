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
use std::time::Duration;

use ureq::Agent;

use crate::{NetworkError, RuntimeError};

pub const OFFLINE_ENV: &str = "RUNEX_OFFLINE";
pub const TIMEOUT_ENV: &str = "RUNEX_HTTP_TIMEOUT_SECS";
pub const DOWNLOAD_TIMEOUT_ENV: &str = "RUNEX_DOWNLOAD_TIMEOUT_SECS";
const USER_AGENT: &str = concat!("lania-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct HttpClient {
    agent: Agent,
    offline: bool,
    download_timeout: Duration,
    github_token: Option<String>,
}

impl HttpClient {
    pub fn new() -> Self {
        let timeout = Duration::from_secs(env_secs(TIMEOUT_ENV, 30));
        let download_timeout = Duration::from_secs(env_secs(DOWNLOAD_TIMEOUT_ENV, 1800));
        let agent: Agent = Agent::config_builder()
            .timeout_connect(Some(timeout))
            .timeout_recv_response(Some(timeout))
            .timeout_recv_body(Some(timeout))
            .user_agent(USER_AGENT)
            .build()
            .into();
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
    /// `std::io::copy` 会“正常”返回已写入的字节数，如果不比对长度，
    /// 会得到一个能被缓存、但解压必然失败的半截产物。
    pub fn download(&self, url: &str, target: &Path) -> Result<u64, RuntimeError> {
        self.ensure_online(url)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let response = self.get_response(url)?;
        let expected = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let mut reader = response.into_body().into_reader();
        let mut file = std::fs::File::create(target)?;
        let written =
            std::io::copy(&mut reader, &mut file).map_err(|error| NetworkError::Request {
                url: url.to_string(),
                detail: error.to_string(),
            })?;
        file.flush()?;
        drop(file);

        if let Some(expected) = expected {
            if written != expected {
                // 半截产物绝不能留在缓存里被下次命中。
                let _ = std::fs::remove_file(target);
                return Err(NetworkError::Request {
                    url: url.to_string(),
                    detail: format!("truncated download: wrote {written} of {expected} bytes"),
                }
                .into());
            }
        }
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
        // 上游偶发 5xx（GitHub 的 504 很常见）不该让整个 install 失败：
        // 只对「可重试」错误退避重试，4xx 立即返回（路径/权限问题重试也没用）。
        const ATTEMPTS: usize = 3;
        let is_github_api = url.contains("api.github.com");
        let mut last: Option<RuntimeError> = None;
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * attempt as u64));
            }
            let mut request = self.agent.get(url);
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

/// 下载缓存路径：`<downloads>/<sha of url>.<file name>`，保证同名不同源的产物不互相覆盖。
pub fn cache_path_for(downloads_dir: &Path, url: &str) -> PathBuf {
    let file_name = url
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("artifact");
    let digest = crate::checksum::hex_digest_sha256(url.as_bytes());
    downloads_dir.join(format!("{}-{file_name}", &digest[..16]))
}

fn env_secs(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
