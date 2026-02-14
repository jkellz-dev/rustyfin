use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio::process::Command;
use tracing::info;

/// Extract a subtitle stream from a media file to a standalone file.
///
/// Uses ffmpeg to extract the subtitle at the given stream index and
/// convert it to the specified output format (typically "vtt" for HLS).
pub async fn extract_subtitle(
    ffmpeg_path: &str,
    input_path: &Path,
    stream_index: i32,
    output_path: &Path,
    output_format: &str,
) -> anyhow::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let codec = match output_format {
        "vtt" | "webvtt" => "webvtt",
        "srt" => "srt",
        "ass" | "ssa" => "ass",
        _ => "webvtt",
    };

    let output = Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-nostdin",
            "-y",
            "-i",
            &input_path.to_string_lossy(),
            "-map",
            &format!("0:s:{stream_index}"),
            "-c:s",
            codec,
            &output_path.to_string_lossy(),
        ])
        .output()
        .await
        .context("failed to spawn ffmpeg for subtitle extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg subtitle extraction failed: {stderr}");
    }

    info!(
        input = %input_path.display(),
        stream_index,
        output = %output_path.display(),
        "subtitle extracted successfully"
    );

    Ok(())
}

/// Get the cache path for an extracted subtitle file.
pub fn subtitle_cache_path(
    temp_base: &Path,
    item_id: &str,
    stream_index: i32,
    format: &str,
) -> PathBuf {
    temp_base
        .join("subtitles")
        .join(format!("{item_id}_{stream_index}.{format}"))
}

/// Determine the content type for a subtitle format.
pub fn subtitle_content_type(format: &str) -> &'static str {
    match format {
        "vtt" | "webvtt" => "text/vtt; charset=utf-8",
        "srt" => "text/plain; charset=utf-8",
        "ass" | "ssa" => "text/plain; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

/// Determine the output format from a request path extension.
pub fn format_from_extension(ext: &str) -> &str {
    match ext.to_lowercase().as_str() {
        "vtt" | "webvtt" => "vtt",
        "srt" => "srt",
        "ass" | "ssa" => "ass",
        _ => "vtt",
    }
}
