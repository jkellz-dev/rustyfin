use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::BaseItemDto;

/// Jellyfin's `ThemeMediaResult`: a `QueryResult` that also names the item that
/// owns the media. Rustyfin has no theme media, so `items` is always empty, but
/// clients still read `OwnerId`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ThemeMediaResult {
    pub items: Vec<BaseItemDto>,
    pub total_record_count: i32,
    pub start_index: i32,
    pub owner_id: Uuid,
}

impl ThemeMediaResult {
    /// An empty result owned by `owner_id`.
    pub fn empty(owner_id: Uuid) -> Self {
        Self {
            items: Vec::new(),
            total_record_count: 0,
            start_index: 0,
            owner_id,
        }
    }
}

/// Jellyfin's `AllThemeMediaResult`, answered by `GET /Items/{id}/ThemeMedia`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AllThemeMediaResult {
    pub theme_videos_result: ThemeMediaResult,
    pub theme_songs_result: ThemeMediaResult,
    pub soundtrack_songs_result: ThemeMediaResult,
}

impl AllThemeMediaResult {
    /// All three results empty, owned by `owner_id`.
    pub fn empty(owner_id: Uuid) -> Self {
        Self {
            theme_videos_result: ThemeMediaResult::empty(owner_id),
            theme_songs_result: ThemeMediaResult::empty(owner_id),
            soundtrack_songs_result: ThemeMediaResult::empty(owner_id),
        }
    }
}
