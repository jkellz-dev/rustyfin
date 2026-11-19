//! `GET`/`PATCH /api/v1/users/{id}/permissions`: what a user may do beyond
//! Jellyfin's policy. For now one thing: link a server to this one
//! (`may_link`, roadmap 16b.2r). Jellyfin's `Policy` is left as Jellyfin has
//! it, so its web client neither sees nor resets these.

use axum::Json;
use axum::extract::{Path, State};
use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_db::repo::users;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Serialize)]
pub struct Permissions {
    /// May link a server to this one (`GET /api/v1/catalog`).
    pub may_link: bool,
}

/// What to change; what is left out stays.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionsPatch {
    #[serde(default)]
    pub may_link: Option<bool>,
}

async fn current(state: &AppState, id: Uuid) -> Result<Permissions, ApiError> {
    let user = users::find_by_id(&state.pool, id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Permissions {
        may_link: user.may_link,
    })
}

/// A user's own, or anyone's for an administrator.
pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Permissions>, ApiError> {
    if auth.user_id != id && !auth.is_administrator {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(current(&state, id).await?))
}

/// Administrators only; answers with the permissions as they now are.
pub async fn patch(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    Json(change): Json<PermissionsPatch>,
) -> Result<Json<Permissions>, ApiError> {
    if let Some(may_link) = change.may_link
        && !users::set_may_link(&state.pool, id, may_link)
            .await
            .map_err(ApiError::Database)?
    {
        return Err(ApiError::NotFound);
    }
    Ok(Json(current(&state, id).await?))
}
