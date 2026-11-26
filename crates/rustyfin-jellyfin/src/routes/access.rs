//! Per-user library access for routes that take an item id.
//!
//! An administrator can limit a user to some libraries (`POST /Users/{id}/Policy`).
//! The list routes filter by that set; every route that names a single item has to
//! apply the same rule. Jellyfin answers 404 for an item the user may not see, so
//! that is what a hidden item gets here too: a 403 would confirm it exists.
use rustyfin_models::data::{Data, MediaItem};

use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// Succeeds when the item exists and `user_id` may see its library
/// (`rustyfin_services::access`).
pub async fn require_item_access<D: Data>(
    state: &AppState<D>,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<(), ApiError> {
    state.services.access().item(user_id, item_id).await?;
    Ok(())
}

/// Drops the items `user_id` may not see, for list routes that pick items from
/// every library (suggestions, similar items, recommendations).
pub async fn retain_accessible<D: Data>(
    state: &AppState<D>,
    user_id: Uuid,
    items: Vec<MediaItem>,
) -> Result<Vec<MediaItem>, ApiError> {
    Ok(state.services.access().retain(user_id, items).await?)
}

/// The caller, for the services' rules that depend on who asks.
pub fn viewer(auth: &crate::auth::middleware::AuthenticatedUser) -> rustyfin_services::Viewer {
    rustyfin_services::Viewer {
        user: auth.user_id,
        is_administrator: auth.is_administrator,
    }
}
