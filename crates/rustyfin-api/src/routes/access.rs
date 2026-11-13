//! Per-user library access for routes that take an item id.
//!
//! An administrator can limit a user to some libraries (`POST /Users/{id}/Policy`).
//! The list routes filter by that set; every route that names a single item has to
//! apply the same rule. Jellyfin answers 404 for an item the user may not see, so
//! that is what a hidden item gets here too: a 403 would confirm it exists.

use uuid::Uuid;

use rustyfin_db::repo::media_items::{self, MediaItemRow};
use rustyfin_db::repo::user_library_access;

use crate::error::ApiError;
use crate::router::AppState;

/// Succeeds when the item exists and `user_id` may see its library.
///
/// Items that belong to no library are visible to everyone, as in `GET /Items/{id}`.
pub async fn require_item_access(
    state: &AppState,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<(), ApiError> {
    let item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let Some(library_id) = item.library_id else {
        return Ok(());
    };
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?;
    match accessible {
        Some(allowed) if !allowed.contains(&library_id) => Err(ApiError::NotFound),
        _ => Ok(()),
    }
}

/// Drops the items `user_id` may not see, for list routes that pick items from
/// every library (suggestions, similar items, recommendations).
pub async fn retain_accessible(
    state: &AppState,
    user_id: Uuid,
    items: Vec<MediaItemRow>,
) -> Result<Vec<MediaItemRow>, ApiError> {
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?;
    let Some(allowed) = accessible else {
        return Ok(items);
    };
    Ok(items
        .into_iter()
        .filter(|item| item.library_id.is_none_or(|lib| allowed.contains(&lib)))
        .collect())
}
