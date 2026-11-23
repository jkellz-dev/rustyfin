//! Owner (Plezzy): show badges count seasons, not unwatched episodes. Every
//! list a client draws posters from must carry UserData, or the client falls
//! back to ChildCount (a show's seasons). /Items/Latest, Similar and
//! Suggestions carried none.
use crate::common;

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

async fn show_with_two_seasons(app: &common::TestApp, lib: Uuid) -> Uuid {
    let series = app
        .create_media_item("Badge Show", "Series", lib, None)
        .await;
    for s in 1..=2 {
        let season = app
            .create_media_item(&format!("Season {s}"), "Season", lib, Some(series))
            .await;
        for e in 1..=3 {
            let ep = app
                .create_media_item(&format!("S{s}E{e}"), "Episode", lib, Some(season))
                .await;
            sqlx::query("UPDATE media_items SET series_id = $2 WHERE id = $1")
                .bind(ep)
                .bind(series)
                .execute(&app.pool)
                .await
                .unwrap();
        }
    }
    sqlx::query("UPDATE media_items SET date_created = NOW() + INTERVAL '1 hour' WHERE id = $1")
        .bind(series)
        .execute(&app.pool)
        .await
        .unwrap();
    series
}

fn find(items: &Value, id: Uuid) -> &Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Id"] == id.simple().to_string())
        .unwrap_or_else(|| panic!("{id} not in {items}"))
}

#[tokio::test]
async fn poster_lists_carry_the_unwatched_episode_count() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("TV Shows", "tvshows").await;
    let series = show_with_two_seasons(&app, lib).await;

    for url in [
        format!("/Items/Latest?ParentId={lib}&IncludeItemTypes=Series"),
        "/Items/Latest?IncludeItemTypes=Series".to_string(),
    ] {
        let (status, body) = app.get(&url, &token).await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        let show = find(&body, series);
        assert_eq!(
            show["UserData"]["UnplayedItemCount"], 6,
            "{url}: six unwatched episodes, not two seasons: {show}"
        );
    }
    app.cleanup().await;
}

/// Plezy's badge is `RecursiveItemCount` (episodes) minus those watched; it
/// falls back to `ChildCount` (seasons) when lists leave RecursiveItemCount out.
#[tokio::test]
async fn library_lists_carry_each_shows_episode_count() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("TV Shows", "tvshows").await;
    let series = show_with_two_seasons(&app, lib).await;
    let (status, body) = app
        .get(
            &format!(
                "/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Series\
                 &Fields=RecursiveItemCount,ChildCount"
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let show = find(&body["Items"], series);
    assert_eq!(show["RecursiveItemCount"], 6, "{show}");
    assert_eq!(show["UserData"]["UnplayedItemCount"], 6, "{show}");
    app.cleanup().await;
}

/// Plezy's Continue Watching is Resume (MediaTypes=Video) plus NextUp with a
/// NextUpDateCutoff: no tracks. A show never started still offers its first
/// episode, as Jellyfin 10.10 does (the cutoff only drops shows last watched
/// before it); DisableFirstEpisode=true leaves those out (owner, 2026-11-15).
#[tokio::test]
async fn continue_watching_keeps_to_video_and_shows_in_progress() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Mixed", "mixed").await;
    let started = show_with_two_seasons(&app, lib).await;
    let untouched = app
        .create_media_item("Untouched Show", "Series", lib, None)
        .await;
    let season = app
        .create_media_item("Season 1", "Season", lib, Some(untouched))
        .await;
    let ep = app
        .create_media_item("U1", "Episode", lib, Some(season))
        .await;
    sqlx::query("UPDATE media_items SET series_id = $2, parent_index_number = 1, index_number = 1 WHERE id = $1")
        .bind(ep)
        .bind(untouched)
        .execute(&app.pool)
        .await
        .unwrap();
    let (s1e1,): (Uuid,) =
        sqlx::query_as("SELECT id FROM media_items WHERE name = 'S1E1' AND series_id = $1")
            .bind(started)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE media_items SET parent_index_number = substr(name, 2, 1)::int, \
         index_number = substr(name, 4, 1)::int WHERE series_id = $1 AND item_type = 'Episode'",
    )
    .bind(started)
    .execute(&app.pool)
    .await
    .unwrap();
    rustyfin_db::repo::user_data::upsert(&app.pool, user, s1e1, 0, true, false, 1)
        .await
        .unwrap();
    let song = app
        .create_media_item("Half a Song", "Audio", lib, None)
        .await;
    rustyfin_db::repo::user_data::upsert(&app.pool, user, song, 10_000_000, false, false, 0)
        .await
        .unwrap();

    let (_, resume) = app
        .get(
            "/UserItems/Resume?Limit=20&MediaTypes=Video&Recursive=true",
            &token,
        )
        .await;
    assert!(
        resume["Items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["Type"] != "Audio"),
        "{resume}"
    );

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();
    let (status, next) = app
        .get(
            &format!(
                "/Shows/NextUp?Limit=20&EnableResumable=false&NextUpDateCutoff={}",
                cutoff.replace('+', "%2B")
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let shows = |next: &serde_json::Value| -> Vec<String> {
        let mut s: Vec<String> = next["Items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["SeriesId"].as_str().unwrap().to_string())
            .collect();
        s.sort();
        s
    };
    let mut both = vec![started.simple().to_string(), untouched.simple().to_string()];
    both.sort();
    assert_eq!(shows(&next), both, "{next}");
    let (_, next) = app
        .get(
            &format!(
                "/Shows/NextUp?Limit=20&NextUpDateCutoff={}&DisableFirstEpisode=true",
                cutoff.replace('+', "%2B")
            ),
            &token,
        )
        .await;
    assert_eq!(shows(&next), [started.to_string()], "{next}");

    // A show last watched before the cutoff is left out; the untouched one stays.
    sqlx::query("UPDATE user_data SET last_played_date = NOW() - INTERVAL '2 years'")
        .execute(&app.pool)
        .await
        .unwrap();
    let (_, next) = app
        .get(
            &format!(
                "/Shows/NextUp?Limit=20&NextUpDateCutoff={}",
                cutoff.replace('+', "%2B")
            ),
            &token,
        )
        .await;
    assert_eq!(shows(&next), [untouched.to_string()], "{next}");
    app.cleanup().await;
}
