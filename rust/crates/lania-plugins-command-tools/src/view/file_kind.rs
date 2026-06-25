use anyhow::{Context, Result};
use std::{fs, path::Path};

use super::{ViewFileKind, BINARY_SNIFF_LIMIT};

pub(super) fn inspect_view_file_kind(path: &Path) -> Result<ViewFileKind> {
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| item.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico"
    ) {
        return Ok(ViewFileKind::Image);
    }
    if matches!(
        extension.as_str(),
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v"
    ) {
        return Ok(ViewFileKind::Video);
    }
    if matches!(
        extension.as_str(),
        "mp3" | "wav" | "m4a" | "flac" | "ogg" | "aac"
    ) {
        return Ok(ViewFileKind::Audio);
    }
    if extension == "pdf" {
        return Ok(ViewFileKind::Pdf);
    }

    let bytes =
        fs::read(path).with_context(|| format!("failed to read file {}", path.display()))?;
    let sample = &bytes[..bytes.len().min(BINARY_SNIFF_LIMIT)];
    if sample.contains(&0) {
        return Ok(ViewFileKind::Binary);
    }
    Ok(ViewFileKind::Text)
}

pub(super) fn view_kind_name(kind: &ViewFileKind) -> &'static str {
    match kind {
        ViewFileKind::Text => "text",
        ViewFileKind::Image => "image",
        ViewFileKind::Video => "video",
        ViewFileKind::Audio => "audio",
        ViewFileKind::Pdf => "pdf",
        ViewFileKind::Binary => "binary",
    }
}
