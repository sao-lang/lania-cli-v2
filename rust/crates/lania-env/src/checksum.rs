//! SHA-256 计算与官方校验和清单解析。

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::VerifyError;

/// 对内存数据计算 SHA-256 十六进制摘要。
pub fn hex_digest_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// 对大文件流式计算 SHA-256 十六进制摘要。
pub fn hex_digest_sha256_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 在官方 checksum 清单里查某个文件名对应的摘要。
///
/// 上游清单格式大同小异（`<hex>  <file>` 或 `<hex> *<file>`，也见过 `SHA256 (<file>) = <hex>`
/// 的 BSD 风格），这里统一解析，避免每种 Provider 各写一份。
pub fn find_in_manifest(manifest: &str, file_name: &str) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((digest, name)) = split_checksum_line(line) else {
            continue;
        };
        let name = name.trim_start_matches('*');
        if name == file_name || name.ends_with(&format!("/{file_name}")) {
            return Some(digest.to_ascii_lowercase());
        }
    }
    None
}

fn split_checksum_line(line: &str) -> Option<(&str, &str)> {
    if let Some(rest) = line.strip_prefix("SHA256 (") {
        let (name, rest) = rest.split_once(')')?;
        let digest = rest.trim().trim_start_matches('=').trim();
        return is_hex(digest).then_some((digest, name));
    }
    let mut parts = line.split_whitespace();
    let digest = parts.next()?;
    let name = parts.next()?;
    is_hex(digest).then_some((digest, name))
}

fn is_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// 校验文件摘要，不匹配时报错并保留原始文件名便于定位。
pub fn verify_file(path: &Path, expected: &str) -> Result<(), VerifyError> {
    let actual = hex_digest_sha256_file(path).map_err(|error| VerifyError::ChecksumMismatch {
        artifact: file_label(path),
        expected: expected.to_string(),
        actual: format!("<unreadable: {error}>"),
    })?;
    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(VerifyError::ChecksumMismatch {
        artifact: file_label(path),
        expected: expected.to_ascii_lowercase(),
        actual,
    })
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_gnu_style_manifest() {
        let manifest = format!("{DIGEST}  node-v20.11.1-linux-x64.tar.gz\n");
        assert_eq!(
            find_in_manifest(&manifest, "node-v20.11.1-linux-x64.tar.gz").as_deref(),
            Some(DIGEST)
        );
        assert!(find_in_manifest(&manifest, "node-v22.0.0-linux-x64.tar.gz").is_none());
    }

    #[test]
    fn parses_bsd_style_manifest() {
        let manifest = format!("SHA256 (file.zip) = {}\n", DIGEST.to_uppercase());
        assert_eq!(
            find_in_manifest(&manifest, "file.zip").as_deref(),
            Some(DIGEST)
        );
    }

    #[test]
    fn ignores_binary_marker_and_paths() {
        let manifest = format!("{DIGEST} *dist/file.zip\n");
        assert_eq!(
            find_in_manifest(&manifest, "file.zip").as_deref(),
            Some(DIGEST)
        );
    }

    #[test]
    fn digest_matches_known_value() {
        assert_eq!(
            hex_digest_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
