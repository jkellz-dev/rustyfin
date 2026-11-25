pub mod activity_log;
pub mod api_keys;
pub mod catalog;
pub mod chapters;
pub mod collections;
pub mod device_auth;
pub mod display_preferences;
pub mod images;
pub mod libraries;
pub mod links;
pub mod media_items;
pub mod media_streams;
pub mod people;
pub mod play_history;
pub mod playlists;
pub mod search_feed;
pub mod sessions;
pub mod status;
pub mod task_results;
pub mod user_data;
pub mod user_library_access;
pub mod users;

/// Rows as models: what a query read, in the shape callers get.
pub(crate) fn all<R, M: From<R>>(rows: Vec<R>) -> Vec<M> {
    rows.into_iter().map(M::from).collect()
}

/// A row, if any, as a model.
pub(crate) fn opt<R, M: From<R>>(row: Option<R>) -> Option<M> {
    row.map(M::from)
}
