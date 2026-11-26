//! `GET /api/v1/catalog`: the change feed a linked Rustyfin syncs from (roadmap
//! 16b.2r). The caller is an ordinary user, signed in the usual way (forward
//! auth), whom the admin lets link (`may_link`); it sees what that user can see.

use axum::Json;
use axum::extract::{Query, State};
use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_models::data::{CatalogStore, Data, UserStore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// Items per page unless the caller asks for fewer (or more, up to `MAX`).
const DEFAULT_LIMIT: i64 = 1000;
const MAX_LIMIT: i64 = 5000;

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    /// The cursor from the last sync's first page; none for everything.
    since: Option<String>,
    /// The `Next` of the page before.
    after: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Changed {
    id: Uuid,
    #[serde(rename = "type")]
    item_type: String,
    library_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct Removed {
    id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct CatalogPage {
    /// Changed (or, with no `since`, all) items; details from `/Items?Ids=`.
    items: Vec<Changed>,
    /// Removed since `since`; on the first page only.
    removed: Vec<Removed>,
    /// Keep the first page's, and pass it as `since` next time.
    cursor: String,
    more: bool,
    /// Pass as `after` for the next page.
    next: Option<String>,
}

/// `GET /api/v1/catalog?since=&after=&limit=`
pub async fn catalog(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(q): Query<CatalogQuery>,
) -> Result<Json<CatalogPage>, ApiError> {
    let user = state
        .data
        .users()
        .find_by_id(auth.user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !user.may_link {
        return Err(ApiError::Forbidden);
    }
    let since = q.since.filter(|s| !s.is_empty());
    if since
        .as_deref()
        .is_some_and(|s| !rustyfin_models::data::is_catalog_snapshot(s))
    {
        return Err(ApiError::BadRequest(
            "since: not a cursor from this feed".into(),
        ));
    }
    let after = match q.after.filter(|s| !s.is_empty()) {
        Some(a) => Some(
            rustyfin_models::data::parse_catalog_after(&a)
                .ok_or_else(|| ApiError::BadRequest("after: not a Next from this feed".into()))?,
        ),
        None => None,
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let libraries = state.services.access().limited_to(user.id).await?;
    let page = state
        .data
        .catalog()
        .changes(
            libraries.as_deref(),
            since.as_deref(),
            after.as_ref(),
            limit,
        )
        .await?;
    Ok(Json(CatalogPage {
        more: page.next.is_some(),
        items: page
            .changes
            .into_iter()
            .map(|c| Changed {
                id: c.id,
                item_type: c.item_type,
                library_id: c.library_id,
            })
            .collect(),
        removed: page.removed.into_iter().map(|id| Removed { id }).collect(),
        cursor: page.snapshot,
        next: page.next,
    }))
}
