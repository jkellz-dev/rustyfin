//! Owner (Plezzy): show badges count seasons, not unwatched episodes. Every
//! list a client draws posters from must carry UserData, or the client falls
//! back to ChildCount (a show's seasons). /Items/Latest, Similar and
//! Suggestions carried none.
#[allow(dead_code)]
mod common;

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

fn find<'a>(items: &'a Value, id: Uuid) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Id"] == id.to_string())
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
