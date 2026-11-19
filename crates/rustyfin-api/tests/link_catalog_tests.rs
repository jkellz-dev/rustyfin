//! The friend's side of a Rustyfin-to-Rustyfin link (roadmap 16b.2r): an
//! ordinary user the admin lets link, and the change feed it syncs from.

#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Ids in a catalogue page's `Items`, as UUIDs.
fn ids(page: &Value, key: &str) -> Vec<Uuid> {
    page[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key}: {page}"))
        .iter()
        .map(|i| i["Id"].as_str().unwrap().parse().unwrap())
        .collect()
}

fn catalog(since: &Value) -> String {
    match since.as_str() {
        Some(c) => format!("/Link/Catalog?since={c}"),
        None => "/Link/Catalog".to_string(),
    }
}

/// Linking is a permission on an ordinary user, off by default and set by an
/// administrator; the admin's Policy edits without it (Jellyfin's own web
/// client sends none) leave it as it is.
#[tokio::test]
async fn an_admin_lets_an_ordinary_user_link() {
    let app = common::TestApp::new().await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (friend, friend_token) = app.create_user("jonathan", false).await;

    let (status, _) = app.get("/Link/Catalog", &friend_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "off by default");
    let (_, me) = app.get(&format!("/Users/{friend}"), &friend_token).await;
    assert_eq!(me["Policy"]["MayLink"], false, "{me}");

    // Only an admin may turn it on.
    let on = json!({"IsAdministrator": false, "EnableAllFolders": true, "MayLink": true});
    let (status, _) = app
        .post(&format!("/Users/{friend}/Policy"), &on, &friend_token)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post(&format!("/Users/{friend}/Policy"), &on, &admin_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, me) = app.get(&format!("/Users/{friend}"), &friend_token).await;
    assert_eq!(me["Policy"]["MayLink"], true, "{me}");
    let (status, _) = app.get("/Link/Catalog", &friend_token).await;
    assert_eq!(status, StatusCode::OK);

    // A policy edit that does not mention it keeps it.
    let edit = json!({"IsAdministrator": false, "EnableAllFolders": true});
    app.post(&format!("/Users/{friend}/Policy"), &edit, &admin_token)
        .await;
    let (_, me) = app.get(&format!("/Users/{friend}"), &friend_token).await;
    assert_eq!(me["Policy"]["MayLink"], true, "{me}");

    let off = json!({"IsAdministrator": false, "EnableAllFolders": true, "MayLink": false});
    app.post(&format!("/Users/{friend}/Policy"), &off, &admin_token)
        .await;
    let (status, _) = app.get("/Link/Catalog", &friend_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// `GET /Link/Catalog`: what changed in the libraries the user can see since a
/// cursor, as ids (the details come from `/Items?Ids=`), with removals. Only
/// this server's own items: never a copy it holds from another link.
#[tokio::test]
async fn the_change_feed_carries_only_what_changed_where_the_user_can_see() {
    let app = common::TestApp::new().await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (friend, token) = app.create_user("jonathan", false).await;
    let movies = app.create_library("Movies", "movies").await;
    let shows = app.create_library("Shows", "tvshows").await;
    let private = app.create_library("Private", "movies").await;
    let film = app.create_media_item("Film", "Movie", movies, None).await;
    let other = app.create_media_item("Other", "Movie", movies, None).await;
    let series = app.create_media_item("Series", "Series", shows, None).await;
    let hidden = app
        .create_media_item("Hidden", "Movie", private, None)
        .await;
    let link_id: Uuid = sqlx::query_scalar(
        "INSERT INTO links (name, kind, url) VALUES ('Alex', 'jellyfin', 'http://x') RETURNING id",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let borrowed = app
        .create_media_item("Borrowed", "Movie", movies, None)
        .await;
    sqlx::query("UPDATE media_items SET link_id = $1, remote_id = 'r1' WHERE id = $2")
        .bind(link_id)
        .bind(borrowed)
        .execute(&app.pool)
        .await
        .unwrap();
    let policy = json!({
        "IsAdministrator": false, "EnableAllFolders": false,
        "EnabledFolders": [movies, shows], "MayLink": true
    });
    let (status, _) = app
        .post(&format!("/Users/{friend}/Policy"), &policy, &admin_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Everything, from the start.
    let (status, first) = app.get("/Link/Catalog", &token).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let mut seen = ids(&first, "Items");
    seen.sort();
    let mut want = vec![film, other, series];
    want.sort();
    assert_eq!(seen, want, "{first}");
    let entry = first["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Id"].as_str() == Some(&series.to_string()))
        .cloned()
        .unwrap();
    assert_eq!(entry["Type"], "Series", "{first}");
    assert_eq!(entry["LibraryId"], json!(shows), "{first}");
    assert_eq!(first["More"], false, "{first}");
    assert!(first["Cursor"].is_string(), "{first}");

    // Nothing changed: nothing comes.
    let (_, quiet) = app.get(&catalog(&first["Cursor"]), &token).await;
    assert!(ids(&quiet, "Items").is_empty(), "{quiet}");
    assert!(ids(&quiet, "Removed").is_empty(), "{quiet}");

    // A renamed film, a new stream on another, a removed one, a hidden change.
    sqlx::query("UPDATE media_items SET name = 'Film (Director''s Cut)' WHERE id = $1")
        .bind(film)
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec) VALUES ($1, 0, 'Video', 'hevc')",
    )
    .bind(other)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM media_items WHERE id = $1")
        .bind(series)
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE media_items SET name = 'Still Hidden' WHERE id = $1")
        .bind(hidden)
        .execute(&app.pool)
        .await
        .unwrap();
    let (_, next) = app.get(&catalog(&quiet["Cursor"]), &token).await;
    let mut changed = ids(&next, "Items");
    changed.sort();
    let mut want = vec![film, other];
    want.sort();
    assert_eq!(changed, want, "{next}");
    assert_eq!(ids(&next, "Removed"), vec![series], "{next}");

    // Paged: a short page says there is more and where to go on from.
    let (_, page) = app.get("/Link/Catalog?limit=1", &token).await;
    assert_eq!(ids(&page, "Items").len(), 1, "{page}");
    assert_eq!(page["More"], true, "{page}");
    let after = page["Next"].as_str().unwrap_or_else(|| panic!("{page}"));
    let (_, rest) = app
        .get(&format!("/Link/Catalog?limit=10&after={after}"), &token)
        .await;
    let mut all = ids(&page, "Items");
    all.extend(ids(&rest, "Items"));
    all.sort();
    let mut want = vec![film, other];
    want.sort();
    assert_eq!(all, want, "{rest}");
    assert_eq!(rest["More"], false, "{rest}");

    let (status, _) = app.get("/Link/Catalog?since=nonsense", &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    app.cleanup().await;
}
