//! Roadmap 11a, from a Plezy session (parity case `plezy.hurl`):
//! Plezy opens a show with `Fields=...,SortName,...,MediaSources` on
//! `/Shows/{id}/Episodes` and on `/Shows/NextUp?SeriesId=`, to badge quality
//! and decide direct play. Rustyfin left `MediaSources` off both routes, and
//! episodes had no `SortName` at all. Jellyfin 10.10.7 sends an episode's
//! numbered sort name (`001 - 0001 - <name>`), a season's `0001`, a track's
//! `0001 - <name>` when it has no sort name of its own.
use crate::common;

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

struct Show {
    series: Uuid,
    season: Uuid,
}

async fn probed_show(app: &common::TestApp) -> Show {
    let lib = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", lib, None).await;
    let season = app
        .create_media_item("Season 1", "Season", lib, Some(series))
        .await;
    sqlx::query(
        "UPDATE media_items SET series_id = $1, index_number = 1, sort_name = NULL WHERE id = $2",
    )
    .bind(series)
    .bind(season)
    .execute(&app.pool)
    .await
    .unwrap();
    for (n, name) in [(1, "Pilot"), (2, "Second")] {
        let ep = app
            .create_media_item(name, "Episode", lib, Some(season))
            .await;
        sqlx::query(
            "UPDATE media_items SET series_id = $2, season_id = $3, parent_index_number = 1, \
             index_number = $4, sort_name = NULL, path = '/media/shows/' || $5 || '.mkv', \
             container = 'mkv', size = 1000, runtime_ticks = 6000000000 WHERE id = $1",
        )
        .bind(ep)
        .bind(series)
        .bind(season)
        .bind(n)
        .bind(name)
        .execute(&app.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, \
             is_default, time_base) VALUES ($1, 0, 'Video', 'h264', 1920, 1080, true, '1/24000')",
        )
        .bind(ep)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    Show { series, season }
}

fn items(body: &Value) -> &Vec<Value> {
    body["Items"].as_array().unwrap_or_else(|| panic!("{body}"))
}

fn assert_has_source(item: &Value) {
    let sources = item["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("MediaSources missing: {item}"));
    assert_eq!(sources.len(), 1, "{item}");
    assert_eq!(sources[0]["Container"], "mkv", "{item}");
}

#[tokio::test]
async fn episodes_carry_media_sources_and_numbered_sort_names_when_asked() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let show = probed_show(&app).await;

    let path = format!(
        "/Shows/{}/Episodes?SeasonId={}&Fields=SortName,Overview,MediaSources",
        show.series, show.season
    );
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = items(&body);
    assert_eq!(list.len(), 2, "{body}");
    for item in list {
        assert_has_source(item);
    }
    assert_eq!(list[0]["SortName"], "001 - 0001 - Pilot", "{body}");
    assert_eq!(list[1]["SortName"], "001 - 0002 - Second", "{body}");

    let (status, body) = app
        .get(
            &format!("/Shows/{}/Seasons?Fields=SortName", show.series),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(items(&body)[0]["SortName"], "0001", "{body}");

    // Without the field, the list stays light.
    let (_, body) = app
        .get(
            &format!("/Shows/{}/Episodes?SeasonId={}", show.series, show.season),
            &token,
        )
        .await;
    assert!(
        items(&body)[0]
            .get("MediaSources")
            .is_none_or(Value::is_null),
        "{body}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn next_up_for_a_show_carries_media_sources_when_asked() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let show = probed_show(&app).await;

    let path = format!(
        "/Shows/NextUp?SeriesId={}&Limit=1&Fields=SortName,MediaSources",
        show.series
    );
    let (status, body) = app.get(&path, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = items(&body);
    assert_eq!(list.len(), 1, "{body}");
    assert_eq!(list[0]["Name"], "Pilot", "{body}");
    assert_has_source(&list[0]);
    assert_eq!(list[0]["SortName"], "001 - 0001 - Pilot", "{body}");
    app.cleanup().await;
}
