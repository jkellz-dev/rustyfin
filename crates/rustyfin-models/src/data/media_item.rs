use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An item in the library: a film, show, season, episode, artist, album,
/// track, folder or extra.
#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: Uuid,
    pub item_type: String,
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub sort_name: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub path: Option<String>,
    pub community_rating: Option<f32>,
    pub critic_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub genres: Option<Vec<String>>,
    pub studios: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub runtime_ticks: Option<i64>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub is_folder: bool,
    pub date_created: DateTime<Utc>,
    pub date_modified: Option<DateTime<Utc>>,
    pub date_last_media_added: Option<DateTime<Utc>>,
    pub series_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub container: Option<String>,
    pub total_bitrate: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: Option<i64>,
    pub etag: Option<String>,
    pub presentation_unique_key: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub extra_data: Option<serde_json::Value>,
    /// A track's own artists from its tags (None: never read).
    pub artists: Option<Vec<String>>,
    /// Jellyfin's `ExtraType` when this is an extra (trailer, featurette, ...).
    pub extra_type: Option<String>,
    /// The film, show or season an extra belongs to.
    pub owner_id: Option<Uuid>,
    /// A track's lyrics were found by the scan (None: not looked at yet).
    pub has_lyrics: Option<bool>,
    /// A linked server's copy (roadmap 16b): the link, and the item's id and
    /// tag there. Linked copies have no path.
    pub link_id: Option<Uuid>,
    pub remote_id: Option<String>,
    pub remote_etag: Option<String>,
}
