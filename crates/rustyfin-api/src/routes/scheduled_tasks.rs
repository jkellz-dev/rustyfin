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
                name: info.name.clone(),
                key: info.key.clone(),
                id: info.key.clone(),
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
            key: info.key.clone(),
            is_hidden: false,
            name: info.name.clone(),
            description: info.description.clone(),
            category: info.category.clone(),
            state: state_str.to_string(),
            current_progress_percentage: None,
            last_execution_result: last_result,
            triggers: triggers_for(&state, &info.schedule_name),
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
            name: info.name.clone(),
            key: info.key.clone(),
            id: info.key.clone(),
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
        key: info.key.clone(),
        is_hidden: false,
        name: info.name.clone(),
        description: info.description.clone(),
        category: info.category.clone(),
        state: state_str.to_string(),
        current_progress_percentage: None,
        last_execution_result: last_result,
        triggers: triggers_for(&state, &info.schedule_name),
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

const TICKS_PER_SECOND: i64 = 10_000_000;

/// A task's schedule as Jellyfin's triggers. Rustyfin schedules by cron
/// (`[tasks.schedules]`); the common shapes have a Jellyfin trigger: a fixed
/// time every day (`DailyTrigger`), on one weekday (`WeeklyTrigger`), or every
/// N minutes or hours (`IntervalTrigger`). Anything else lists no trigger
/// rather than a wrong one.
fn triggers_for(state: &AppState, schedule_name: &str) -> Vec<TaskTriggerDto> {
    state.scan_schedules["Schedules"][schedule_name]
        .as_str()
        .and_then(trigger_for_cron)
        .into_iter()
        .collect()
}

fn trigger_for_cron(cron: &str) -> Option<TaskTriggerDto> {
    let f: Vec<&str> = cron.split_whitespace().collect();
    if !(6..=7).contains(&f.len()) || f[3] != "*" || f[4] != "*" {
        return None;
    }
    if f.len() == 7 && f[6] != "*" {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let every = |s: &str| s.strip_prefix("*/").and_then(|n| n.parse::<i64>().ok());
    let trigger = |kind: &str| TaskTriggerDto {
        trigger_type: kind.to_string(),
        time_of_day_ticks: None,
        interval_ticks: None,
        max_runtime_ticks: None,
        day_of_week: None,
    };
    let (sec, min, hour, dow) = (f[0], f[1], f[2], f[5]);
    if let (Some(s), Some(m), Some(h)) = (num(sec), num(min), num(hour)) {
        let time = Some((h * 3600 + m * 60 + s) * TICKS_PER_SECOND);
        if dow == "*" || dow == "?" {
            return Some(TaskTriggerDto {
                time_of_day_ticks: time,
                ..trigger("DailyTrigger")
            });
        }
        return weekday(dow).map(|day| TaskTriggerDto {
            time_of_day_ticks: time,
            day_of_week: Some(day.to_string()),
            ..trigger("WeeklyTrigger")
        });
    }
    if dow != "*" && dow != "?" {
        return None;
    }
    let interval = match (num(sec), min, hour) {
        (Some(_), m, "*") => every(m).map(|n| n * 60),
        (Some(_), m, h) if num(m).is_some() => every(h).map(|n| n * 3600),
        _ => None,
    }?;
    Some(TaskTriggerDto {
        interval_ticks: Some(interval * TICKS_PER_SECOND),
        ..trigger("IntervalTrigger")
    })
}

/// A cron weekday (1 = Sunday … 7 = Saturday, or its name) as .NET's `DayOfWeek`.
fn weekday(field: &str) -> Option<&'static str> {
    const DAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    if let Ok(n) = field.parse::<usize>() {
        return (1..=7).contains(&n).then(|| DAYS[n - 1]);
    }
    let lower = field.to_ascii_lowercase();
    (lower.len() >= 3)
        .then(|| {
            DAYS.into_iter()
                .find(|d| d.to_ascii_lowercase().starts_with(&lower))
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600 * TICKS_PER_SECOND;

    #[test]
    fn a_daily_cron_is_a_daily_trigger() {
        let t = trigger_for_cron("0 0 3 * * * *").unwrap();
        assert_eq!(t.trigger_type, "DailyTrigger");
        assert_eq!(t.time_of_day_ticks, Some(3 * HOUR));
    }

    #[test]
    fn a_weekday_cron_is_a_weekly_trigger() {
        let t = trigger_for_cron("0 30 4 * * 7 *").unwrap();
        assert_eq!(t.trigger_type, "WeeklyTrigger");
        assert_eq!(t.day_of_week.as_deref(), Some("Saturday"));
        assert_eq!(
            t.time_of_day_ticks,
            Some(4 * HOUR + 30 * 60 * TICKS_PER_SECOND)
        );
        let t = trigger_for_cron("0 0 1 * * Mon").unwrap();
        assert_eq!(t.day_of_week.as_deref(), Some("Monday"));
    }

    #[test]
    fn a_stepped_cron_is_an_interval_trigger() {
        let t = trigger_for_cron("0 */30 * * * * *").unwrap();
        assert_eq!(t.trigger_type, "IntervalTrigger");
        assert_eq!(t.interval_ticks, Some(HOUR / 2));
        let t = trigger_for_cron("0 0 */12 * * *").unwrap();
        assert_eq!(t.interval_ticks, Some(12 * HOUR));
    }

    #[test]
    fn other_crons_list_no_trigger() {
        for cron in [
            "0 0 3 1 * * *",
            "0 0 3 * * 1-5 *",
            "0 0 3,15 * * *",
            "nonsense",
        ] {
            assert!(trigger_for_cron(cron).is_none(), "{cron}");
        }
    }
}
