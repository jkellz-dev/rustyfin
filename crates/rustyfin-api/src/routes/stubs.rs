use axum::Json;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_models::dto::{BaseItemDto, QueryResult};

/// Empty JSON array — no auth required.
pub async fn empty_array() -> Json<Vec<()>> {
    Json(Vec::new())
}

/// Empty `QueryResult` — requires auth.
pub async fn empty_query_result_auth(_auth: AuthenticatedUser) -> Json<QueryResult<BaseItemDto>> {
    Json(QueryResult::empty())
}

/// Empty JSON array — requires auth.
pub async fn empty_array_auth(_auth: AuthenticatedUser) -> Json<Vec<BaseItemDto>> {
    Json(Vec::new())
}

/// Empty JSON array — requires admin.
pub async fn empty_array_admin(_auth: AdminUser) -> Json<Vec<()>> {
    Json(Vec::new())
}
