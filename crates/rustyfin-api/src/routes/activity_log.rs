use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use rustyfin_auth::middleware::AdminUser;
use rustyfin_data::repo::activity_log;
use rustyfin_models::dto::{ActivityLogEntryDto, QueryResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogQuery {
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub min_date: Option<DateTime<Utc>>,
    pub has_user_id: Option<bool>,
}

/// `GET /System/ActivityLog/Entries` — List activity log entries (admin only).
pub async fn get_log_entries(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<ActivityLogQuery>,
) -> Result<Json<QueryResult<ActivityLogEntryDto>>, ApiError> {
    let start_index = query.start_index.unwrap_or(0);
    let limit = query.limit.unwrap_or(25).min(100);

    let rows = activity_log::list_paginated(
        &state.pool,
        start_index,
        limit,
        query.min_date,
        query.has_user_id,
    )
    .await
    .map_err(ApiError::Database)?;

    let total = activity_log::count(&state.pool, query.min_date, query.has_user_id)
        .await
        .map_err(ApiError::Database)?;

    let dtos: Vec<ActivityLogEntryDto> = rows
        .into_iter()
        .map(|row| ActivityLogEntryDto {
            id: row.id,
            name: row.name,
            overview: row.overview,
            log_type: row.log_type,
            severity: row.severity,
            user_id: row.user_id,
            item_id: row.item_id,
            date: row.created_at,
        })
        .collect();

    let mut result = QueryResult::new(dtos, total as i32);
    result.start_index = start_index as i32;
    Ok(Json(result))
}
