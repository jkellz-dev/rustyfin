//! `GET`/`PATCH /api/v1/users/{id}/permissions`: what a user may do beyond
//! Jellyfin's policy. For now one thing: link a server to this one
//! (`may_link`, roadmap 16b.2r). Jellyfin's `Policy` is left as Jellyfin has
//! it, so its web client neither sees nor resets these.
use rustyfin_models::data::Data;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::caller::{Admin, Caller};
use crate::error::ApiError;
use crate::state::ApiState;

#[derive(Debug, Serialize)]
pub struct Permissions {
    /// May link a server to this one (`GET /api/v1/catalog`).
    pub may_link: bool,
}

/// One user's permissions, in the list for the admin.
#[derive(Debug, Serialize)]
pub struct UserPermissions {
    #[serde(serialize_with = "crate::ids::simple")]
    pub id: Uuid,
    pub may_link: bool,
}

/// `GET /api/v1/users/permissions`: every user's, for the admin's users list
/// (which marks server accounts). Administrators only.
pub async fn list<D: Data>(
    State(state): State<ApiState<D>>,
    _admin: Admin,
) -> Result<Json<Vec<UserPermissions>>, ApiError> {
    let users = state.services.users().list().await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserPermissions {
                id: u.id,
                may_link: u.may_link,
            })
            .collect(),
    ))
}

/// What to change; what is left out stays.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionsPatch {
    #[serde(default)]
    pub may_link: Option<bool>,
}

impl From<rustyfin_services::users::Permissions> for Permissions {
    fn from(p: rustyfin_services::users::Permissions) -> Self {
        Self {
            may_link: p.may_link,
        }
    }
}

/// A user's own, or anyone's for an administrator.
pub async fn get<D: Data>(
    State(state): State<ApiState<D>>,
    auth: Caller,
    Path(id): Path<Uuid>,
) -> Result<Json<Permissions>, ApiError> {
    let permissions = state
        .services
        .users()
        .permissions(auth.viewer(), id)
        .await?;
    Ok(Json(permissions.into()))
}

/// Administrators only; answers with the permissions as they now are.
pub async fn patch<D: Data>(
    State(state): State<ApiState<D>>,
    _admin: Admin,
    Path(id): Path<Uuid>,
    Json(change): Json<PermissionsPatch>,
) -> Result<Json<Permissions>, ApiError> {
    let permissions = state
        .services
        .users()
        .set_permissions(id, change.may_link)
        .await?;
    if change.may_link.is_some() {
        // Requests from that user's sessions are scoped anew.
        rustyfin_services::link::scope::forget();
    }
    Ok(Json(permissions.into()))
}
