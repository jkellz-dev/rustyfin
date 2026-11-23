//! Routes the owner chose to serve after the parity catalog (2026-11-22):
//! sharing a playlist (Jellyfin 10.9+), the old `/PlayingItems` reports, the
//! Live TV stream open and close, and asking a session to show an item.
//!
//! Creating a playlist also took `Ids` only as a comma-separated string, while
//! Jellyfin's body (and its clients) send an array: such a request was refused.

use crate::common;

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

const MINUTE: i64 = 600_000_000;

#[tokio::test]
async fn a_playlist_is_shared_and_edited_like_jellyfin() {
    let app = common::TestApp::new().await;
    let (owner_id, owner) = app.create_user("owner", false).await;
    let (friend_id, friend) = app.create_user("friend", false).await;
    let (_, stranger) = app.create_user("stranger", false).await;
    let lib = app.create_library("Music", "music").await;
    let one = app.create_media_item("One", "Audio", lib, None).await;
    let two = app.create_media_item("Two", "Audio", lib, None).await;

    let (status, made) = app
        .post(
            "/Playlists",
            &json!({"Name": "Mix", "Ids": [one], "MediaType": "Audio", "UserId": owner_id.simple().to_string()}),
            &owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    let id = made["Id"].as_str().expect("playlist id").to_string();

    let (status, list) = app.get(&format!("/Playlists/{id}"), &owner).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        list,
        json!({"OpenAccess": false, "Shares": [], "ItemIds": [one.simple().to_string()]})
    );

    // Someone it is not shared with does not see it.
    let (status, _) = app.get(&format!("/Playlists/{id}"), &friend).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, out) = app
        .post(
            &format!("/Playlists/{id}/Users/{friend_id}"),
            &json!({"CanEdit": false}),
            &owner,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{out}");
    let (status, _) = app.get(&format!("/Playlists/{id}"), &friend).await;
    assert_eq!(status, StatusCode::OK);
    let (_, users) = app.get(&format!("/Playlists/{id}/Users"), &owner).await;
    assert_eq!(
        users,
        json!([{"UserId": friend_id.simple().to_string(), "CanEdit": false}])
    );
    // Only the owner lists who it is shared with.
    let (status, _) = app.get(&format!("/Playlists/{id}/Users"), &friend).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, me) = app
        .get(&format!("/Playlists/{id}/Users/{friend_id}"), &friend)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        me,
        json!({"UserId": friend_id.simple().to_string(), "CanEdit": false})
    );
    let (_, me) = app
        .get(&format!("/Playlists/{id}/Users/{owner_id}"), &owner)
        .await;
    assert_eq!(
        me,
        json!({"UserId": owner_id.simple().to_string(), "CanEdit": true})
    );

    // A reader may not change it.
    let (status, _) = app
        .post(
            &format!("/Playlists/{id}"),
            &json!({"Name": "Mine"}),
            &friend,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The owner renames it, replaces its tracks, opens it to all, and lets the friend edit.
    let (status, out) = app
        .post(
            &format!("/Playlists/{id}"),
            &json!({
                "Name": "Renamed",
                "Ids": [two],
                "IsPublic": true,
                "Users": [{"UserId": friend_id.simple().to_string(), "CanEdit": true}],
            }),
            &owner,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{out}");
    let (_, list) = app.get(&format!("/Playlists/{id}"), &stranger).await;
    assert_eq!(list["OpenAccess"], true, "{list}");
    assert_eq!(list["ItemIds"], json!([two.simple().to_string()]), "{list}");
    assert_eq!(
        list["Shares"],
        json!([{"UserId": friend_id.simple().to_string(), "CanEdit": true}])
    );
    let name: (String,) = sqlx::query_as("SELECT name FROM playlists WHERE id = $1")
        .bind(id.parse::<Uuid>().unwrap())
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(name.0, "Renamed");

    // An editor may change it, and may take the share away.
    let (status, _) = app
        .post(
            &format!("/Playlists/{id}"),
            &json!({"Name": "Ours"}),
            &friend,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .delete(&format!("/Playlists/{id}/Users/{friend_id}"), &friend)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, users) = app.get(&format!("/Playlists/{id}/Users"), &owner).await;
    assert_eq!(users, json!([]));
    let (status, _) = app
        .delete(&format!("/Playlists/{id}/Users/{friend_id}"), &owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no such share");
    let (status, _) = app
        .post(
            &format!("/Playlists/{id}/Users/{}", Uuid::new_v4()),
            &json!({"CanEdit": true}),
            &owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no such user");
    app.cleanup().await;
}

#[tokio::test]
async fn the_old_playing_items_reports_keep_the_place() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query("UPDATE media_items SET runtime_ticks = $2 WHERE id = $1")
        .bind(film)
        .bind(100 * MINUTE)
        .execute(&app.pool)
        .await
        .unwrap();

    let (status, _) = app
        .post(
            &format!("/PlayingItems/{film}?canSeek=true"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .post(
            &format!(
                "/PlayingItems/{film}/Progress?positionTicks={}",
                30 * MINUTE
            ),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, item) = app
        .get(&format!("/Users/{user}/Items/{film}"), &token)
        .await;
    assert_eq!(
        item["UserData"]["PlaybackPositionTicks"],
        30 * MINUTE,
        "{item}"
    );

    // The obsolete per-user forms do the same.
    let (status, _) = app
        .post(
            &format!(
                "/Users/{user}/PlayingItems/{film}/Progress?positionTicks={}",
                40 * MINUTE
            ),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, item) = app
        .get(&format!("/Users/{user}/Items/{film}"), &token)
        .await;
    assert_eq!(
        item["UserData"]["PlaybackPositionTicks"],
        40 * MINUTE,
        "{item}"
    );

    // Stopped past the credits: played.
    let (status, _) = app
        .delete(
            &format!("/PlayingItems/{film}?positionTicks={}", 99 * MINUTE),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, item) = app
        .get(&format!("/Users/{user}/Items/{film}"), &token)
        .await;
    assert_eq!(item["UserData"]["Played"], true, "{item}");
    let (status, _) = app
        .delete(&format!("/Users/{user}/PlayingItems/{film}"), &token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.cleanup().await;
}

#[tokio::test]
async fn live_streams_and_display_content_answer() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    // Rustyfin hands out no open tokens: there is never a stream to open.
    let (status, body) = app
        .post("/LiveStreams/Open?openToken=x", &json!({}), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["status"], 404, "{body}");
    let (status, _) = app
        .post("/LiveStreams/Close?liveStreamId=x", &json!({}), &token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = app
        .post(
            &format!(
                "/Sessions/{}/Viewing?itemType=Movie&itemId={film}&itemName=Film",
                Uuid::new_v4()
            ),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.cleanup().await;
}
