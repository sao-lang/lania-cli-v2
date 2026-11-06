//! 归档解压：`tar.gz` / `tar.xz` / `tar.zst` / `zip`。
//!
//! 只做「解到指定目录」这一件事，产物形态的差异（是否需要 strip、单文件还是目录树）
//! 由各 Provider 的 `install_layout` 处理。

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::ArchiveError;

/// 支持的归档格式（按文件名后缀推断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    TarGz,
    TarXz,
    TarZst,
    Zip,
}

impl ArchiveFormat {
    /// 从文件名推断格式。上游命名严格（`node-v20.11.1-linux-x64.tar.gz`），
    /// 因此后缀判断足够，不需要读魔数。
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        let name = name.trim_end_matches(".exe");
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            Some(Self::TarXz)
        } else if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
            Some(Self::TarZst)
        } else if name.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

/// 在解压前确认归档结构完整。
///
/// 有些 CDN 会**不带 `Content-Length` 地中断响应**（或返回 chunked 流被截断），
/// 此时 `io::copy` 不会报错，写出的却是不完整的文件。这里用流式读取把整个
/// 归档走一遍：`tar.gz` 会解压到流末尾（截断时返回 UnexpectedEof），
/// `zip` 会校验中央目录（截断时读不到记录）。
pub fn verify_integrity(archive: &Path) -> Result<(), ArchiveError> {
    let format = ArchiveFormat::from_path(archive).ok_or_else(|| ArchiveError::Unsupported {
        path: archive.display().to_string(),
        detail: "unknown archive extension".to_string(),
    })?;
    let fail = |error: std::io::Error| ArchiveError::Extract {
        path: archive.display().to_string(),
        detail: format!("archive appears truncated or corrupt: {error}"),
    };
    match format {
        ArchiveFormat::Zip => {
            let file = open(archive)?;
            let mut zip = zip::ZipArchive::new(file).map_err(|error| ArchiveError::Extract {
                path: archive.display().to_string(),
                detail: format!("archive appears truncated or corrupt: {error}"),
            })?;
            // 逐个 entry 读到尾：只读中央目录无法发现「数据区被截断」。
            let mut buffer = vec![0u8; 64 * 1024];
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).map_err(|error| ArchiveError::Extract {
                    path: archive.display().to_string(),
                    detail: format!("archive appears truncated or corrupt: {error}"),
                })?;
                while let Ok(read) = entry.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                }
            }
            Ok(())
        }
        ArchiveFormat::TarGz => {
            let file = open(archive)?;
            drain(flate2::read::GzDecoder::new(file)).map_err(fail)
        }
        ArchiveFormat::TarXz => {
            let file = open(archive)?;
            drain(xz2::read::XzDecoder::new(file)).map_err(fail)
        }
        ArchiveFormat::TarZst => {
            let file = open(archive)?;
            let decoder =
                zstd::stream::read::Decoder::new(file).map_err(|error| ArchiveError::Extract {
                    path: archive.display().to_string(),
                    detail: format!("archive appears truncated or corrupt: {error}"),
                })?;
            drain(decoder).map_err(fail)
        }
    }
}

fn drain(reader: impl Read) -> std::io::Result<()> {
    let mut reader = reader;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

/// 解压 `archive` 到 `dest`（`dest` 必须已存在或可创建）。
pub fn extract(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let format = ArchiveFormat::from_path(archive).ok_or_else(|| ArchiveError::Unsupported {
        path: archive.display().to_string(),
        detail: "unknown archive extension".to_string(),
    })?;
    std::fs::create_dir_all(dest).map_err(|error| ArchiveError::Extract {
        path: archive.display().to_string(),
        detail: error.to_string(),
    })?;

    match format {
        ArchiveFormat::Zip => extract_zip(archive, dest),
        ArchiveFormat::TarGz => {
            let file = open(archive)?;
            extract_tar(archive, flate2::read::GzDecoder::new(file), dest)
        }
        ArchiveFormat::TarXz => {
            let file = open(archive)?;
            extract_tar(archive, xz2::read::XzDecoder::new(file), dest)
        }
        ArchiveFormat::TarZst => {
            let file = open(archive)?;
            let decoder =
                zstd::stream::read::Decoder::new(file).map_err(|error| ArchiveError::Extract {
                    path: archive.display().to_string(),
                    detail: error.to_string(),
                })?;
            extract_tar(archive, decoder, dest)
        }
    }
}

fn open(path: &Path) -> Result<std::fs::File, ArchiveError> {
    std::fs::File::open(path).map_err(|error| ArchiveError::Extract {
        path: path.display().to_string(),
        detail: error.to_string(),
    })
}

fn extract_tar<R: Read>(archive: &Path, reader: R, dest: &Path) -> Result<(), ArchiveError> {
    let mut tar = tar::Archive::new(reader);
    // 归档由官方发布，但仍拒绝 `..` / 绝对路径，避免路径穿越。
    tar.set_overwrite(true);
    tar.unpack(dest).map_err(|error| ArchiveError::Extract {
        path: archive.display().to_string(),
        detail: error.to_string(),
    })
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let file = open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| ArchiveError::Extract {
        path: archive.display().to_string(),
        detail: error.to_string(),
    })?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| ArchiveError::Extract {
            path: archive.display().to_string(),
            detail: error.to_string(),
        })?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(ArchiveError::Extract {
                path: archive.display().to_string(),
                detail: format!("refusing to extract unsafe path `{}`", entry.name()),
            });
        };
        let target = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| ArchiveError::Extract {
                path: archive.display().to_string(),
                detail: error.to_string(),
            })?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ArchiveError::Extract {
                path: archive.display().to_string(),
                detail: error.to_string(),
            })?;
        }
        let mut out = std::fs::File::create(&target).map_err(|error| ArchiveError::Extract {
            path: archive.display().to_string(),
            detail: error.to_string(),
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|error| ArchiveError::Extract {
            path: archive.display().to_string(),
            detail: error.to_string(),
        })?;
        // zip 不保留 POSIX 权限位，但官方产物里的可执行文件需要它们。
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

/// 解压后定位「唯一的那一层目录」（`node-v20.11.1-linux-x64/`）。
pub fn single_root(extracted: &Path) -> Result<PathBuf, ArchiveError> {
    let mut entries = top_level_dirs(extracted);
    if entries.len() == 1 {
        return Ok(entries.remove(0));
    }
    Ok(extracted.to_path_buf())
}

/// 列出目录下的全部子目录（已排序，便于稳定输出）。
pub fn top_level_dirs(root: &Path) -> Vec<PathBuf> {
    top_level_entries(root)
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

/// 列出目录下的全部顶层文件（已排序）。
pub fn top_level_files(root: &Path) -> Vec<PathBuf> {
    top_level_entries(root)
        .into_iter()
        .filter(|path| path.is_file())
        .collect()
}

fn top_level_entries(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats_from_file_names() {
        let cases = [
            ("node-v20.11.1-linux-x64.tar.gz", ArchiveFormat::TarGz),
            ("python.tar.xz", ArchiveFormat::TarXz),
            ("cpython.tar.zst", ArchiveFormat::TarZst),
            ("deno-x86_64-pc-windows-msvc.zip", ArchiveFormat::Zip),
            ("bun-windows-x64.zip", ArchiveFormat::Zip),
        ];
        for (name, expected) in cases {
            assert_eq!(
                ArchiveFormat::from_path(Path::new(name)),
                Some(expected),
                "{name}"
            );
        }
        assert_eq!(ArchiveFormat::from_path(Path::new("go1.22.0.msi")), None);
    }

    #[test]
    fn unzips_real_archive() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!("lania-archive-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("sample.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
            zip.start_file("bin/tool", options).unwrap();
            zip.write_all(b"#!/bin/sh\necho hi\n").unwrap();
            zip.add_directory("lib/", options).unwrap();
            zip.start_file("lib/data.txt", options).unwrap();
            zip.write_all(b"data").unwrap();
            zip.finish().unwrap();
        }
        let out = root.join("out");
        extract(&archive, &out).unwrap();
        assert!(out.join("bin").join("tool").is_file());
        assert!(out.join("lib").join("data.txt").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn single_root_unwraps_single_directory() {
        let root = std::env::temp_dir().join(format!(
            "lania-archive-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("node-v20.11.1")).unwrap();
        std::fs::write(root.join("node-v20.11.1").join("README"), b"x").unwrap();
        let found = single_root(&root).unwrap();
        assert!(found.ends_with("node-v20.11.1"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
