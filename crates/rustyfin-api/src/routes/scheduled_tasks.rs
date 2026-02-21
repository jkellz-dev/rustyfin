use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use rustyfin_auth::middleware::AdminUser;
use rustyfin_db::repo::task_results;
use rustyfin_models::dto::{TaskInfoDto, TaskResultDto, TaskTriggerDto};

use crate::error::ApiError;
use crate::router::AppState;

pub async fn get_tasks(
    State(state): State<AppState>,
    _auth: AdminUser,
) -> Result<Json<Vec<TaskInfoDto>>, ApiError> {
    let Some(ref scheduler) = state.task_scheduler else {
        return Ok(Json(Vec::new()));
    };

    let task_infos = scheduler.list_task_infos().await;
    let mut dtos = Vec::with_capacity(task_infos.len());

    for info in &task_infos {
        let last_result = task_results::latest_by_task_key(&state.pool, &info.key)
            .await
            .ok()
            .flatten()
            .map(|r| TaskResultDto {
                start_time_utc: r.started_at.to_rfc3339(),
                end_time_utc: r.ended_at.map(|t| t.to_rfc3339()),
                status: r.status,
                error_message: r.error_message,
            });

        let state_str = if let Some(ref lr) = last_result {
            if lr.status == "Running" {
                "Running"
            } else {
                "Idle"
            }
        } else {
            "Idle"
        };

        dtos.push(TaskInfoDto {
            id: info.key.clone(),
            name: info.name.clone(),
            description: info.description.clone(),
            category: info.category.clone(),
            state: state_str.to_string(),
            current_progress_percentage: None,
            last_execution_result: last_result,
            triggers: vec![TaskTriggerDto {
                trigger_type: "IntervalTrigger".to_string(),
                time_of_day_ticks: None,
                interval_ticks: None,
                max_runtime_ticks: None,
            }],
        });
    }

    Ok(Json(dtos))
}

pub async fn get_task(
    State(state): State<AppState>,
    _auth: AdminUser,
    Path(task_id): Path<String>,
) -> Result<Json<TaskInfoDto>, ApiError> {
    let Some(ref scheduler) = state.task_scheduler else {
        return Err(ApiError::NotFound);
    };

    let task_infos = scheduler.list_task_infos().await;
    let info = task_infos
        .iter()
        .find(|t| t.key == task_id)
        .ok_or(ApiError::NotFound)?;

    let last_result = task_results::latest_by_task_key(&state.pool, &info.key)
        .await
        .ok()
        .flatten()
        .map(|r| TaskResultDto {
            start_time_utc: r.started_at.to_rfc3339(),
            end_time_utc: r.ended_at.map(|t| t.to_rfc3339()),
            status: r.status,
            error_message: r.error_message,
        });

    let state_str = if let Some(ref lr) = last_result {
        if lr.status == "Running" {
            "Running"
        } else {
            "Idle"
        }
    } else {
        "Idle"
    };

    Ok(Json(TaskInfoDto {
        id: info.key.clone(),
        name: info.name.clone(),
        description: info.description.clone(),
        category: info.category.clone(),
        state: state_str.to_string(),
        current_progress_percentage: None,
        last_execution_result: last_result,
        triggers: vec![TaskTriggerDto {
            trigger_type: "IntervalTrigger".to_string(),
            time_of_day_ticks: None,
            interval_ticks: None,
            max_runtime_ticks: None,
        }],
    }))
}

pub async fn start_task(
    State(state): State<AppState>,
    _auth: AdminUser,
    Path(task_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let Some(ref scheduler) = state.task_scheduler else {
        return Err(ApiError::NotFound);
    };

    if scheduler.run_task_by_key(&task_id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

pub async fn stop_task(_auth: AdminUser, Path(_task_id): Path<String>) -> StatusCode {
    // Task cancellation not supported -- return 204 to satisfy client
    StatusCode::NO_CONTENT
}
