//! Lists honour the libraries a user is limited to. Next up and upcoming
//! episodes: before, a user limited to one library was offered the first episode of
//! every unstarted show anywhere, could ask for any show's next episode by
//! its id, and saw every library's upcoming episodes.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::{user_library_access, users};
use uuid::Uuid;

/// A show with one season and two episodes, the first airing in the future.
async fn show(app: &common::TestApp, lib: Uuid, name: &str) -> Uuid {
    let series = app.create_media_item(name, "Series", lib, None).await;
    let season = app
        .create_media_item("Season 1", "Season", lib, Some(series))
        .await;
    for e in 1..=2 {
        let ep = app
            .create_media_item(&format!("{name} {e}"), "Episode", lib, Some(season))
            .await;
        sqlx::query(
            "UPDATE media_items SET series_id = $2, season_id = $3, parent_index_number = 1, \
             index_number = $4, premiere_date = CURRENT_DATE + 7 WHERE id = $1",
        )
        .bind(ep)
        .bind(series)
        .bind(season)
        .bind(e)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    series
}

fn names(body: &serde_json::Value) -> Vec<String> {
    body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn next_up_and_upcoming_leave_out_libraries_the_user_may_not_see() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let open = app.create_library("Shows", "tvshows").await;
    let closed = app.create_library("Kept", "tvshows").await;
    show(&app, open, "Seen").await;
    let hidden = show(&app, closed, "Hidden").await;
    users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user, &[open])
        .await
        .unwrap();

    let (status, body) = app.get("/Shows/NextUp", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body), ["Seen 1"], "{body}");
    assert_eq!(body["TotalRecordCount"], 1, "{body}");

    let (_, body) = app
        .get(&format!("/Shows/NextUp?SeriesId={hidden}"), &token)
        .await;
    assert!(names(&body).is_empty(), "{body}");

    let (status, body) = app.get("/Shows/Upcoming", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut upcoming = names(&body);
    upcoming.sort();
    assert_eq!(upcoming, ["Seen 1", "Seen 2"], "{body}");
    assert_eq!(body["TotalRecordCount"], 2, "{body}");
    app.cleanup().await;
}

/// Latest across libraries for a user limited to some: the newest of what
/// they may see. Before, the newest of everything was read and then thinned,
/// so newer titles elsewhere left them with fewer or none.
#[tokio::test]
async fn latest_fills_up_from_the_libraries_the_user_may_see() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let open = app.create_library("Films", "movies").await;
    let closed = app.create_library("Kept", "movies").await;
    for n in 1..=3 {
        app.create_media_item(&format!("Seen {n}"), "Movie", open, None)
            .await;
    }
    for n in 1..=5 {
        let id = app
            .create_media_item(&format!("Hidden {n}"), "Movie", closed, None)
            .await;
        sqlx::query("UPDATE media_items SET date_created = NOW() + interval '1 day' WHERE id = $1")
            .bind(id)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    user_library_access::set_enabled_libraries(&app.pool, user, &[open])
        .await
        .unwrap();

    let (status, body) = app
        .get("/Items/Latest?Limit=3&GroupItems=false", &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut got: Vec<String> = body
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    got.sort();
    assert_eq!(got, ["Seen 1", "Seen 2", "Seen 3"], "{body}");
    app.cleanup().await;
}
