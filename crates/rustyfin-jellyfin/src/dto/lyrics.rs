//! `GET /Audio/{itemId}/Lyrics` (Jellyfin's `LyricDto`).

use serde::Serialize;

/// A track's lyrics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LyricDto {
    pub metadata: LyricMetadata,
    pub lyrics: Vec<LyricLineDto>,
}

/// Jellyfin 10.10 sends this empty, even for LRC files with `[ar:]` tags.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LyricMetadata {}

/// One line; `start` (ticks) only for synced lyrics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LyricLineDto {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
}
