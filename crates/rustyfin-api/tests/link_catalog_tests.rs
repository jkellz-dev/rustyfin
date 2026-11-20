//! The friend's side of a Rustyfin-to-Rustyfin link (roadmap 16b.2r): an
//! ordinary user the admin lets link, and the change feed it syncs from.
//! Both are Rustyfin's own API (`/api/v1`, owner 2026-11-19), apart from the
//! Jellyfin routes: snake_case JSON and query keys.

#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Ids in a catalogue page's `key` list, as UUIDs.
fn ids(page: &Value, key: &str) -> Vec<Uuid> {
    page[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key}: {page}"))
        .iter()
        .map(|i| i["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

fn catalog(since: &Value) -> String {
    match since.as_str() {
        Some(c) => format!("/api/v1/catalog?since={c}"),
        None => "/api/v1/catalog".to_string(),
    }
}

/// Linking is a permission on an ordinary user, off by default, set by an
/// administrator through Rustyfin's own API; the Jellyfin routes and their
/// `Policy` are left as Jellyfin has them.
#[tokio::test]
async fn an_admin_lets_an_ordinary_user_link() {
    let app = common::TestApp::new().await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (friend, friend_token) = app.create_user("jonathan", false).await;
    let permissions = format!("/api/v1/users/{friend}/permissions");

    let (status, _) = app.get("/api/v1/catalog", &friend_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "off by default");
    let (status, mine) = app.get(&permissions, &friend_token).await;
    assert_eq!(status, StatusCode::OK, "a user sees their own: {mine}");
    assert_eq!(mine["may_link"], false, "{mine}");
    let (_, me) = app.get(&format!("/Users/{friend}"), &friend_token).await;
    assert!(
        me["Policy"].get("MayLink").is_none(),
        "not in Jellyfin's Policy: {me}"
    );

    // Only an admin may turn it on.
    let on = json!({"may_link": true});
    let (status, _) = app.patch(&permissions, &on, &friend_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, now) = app.patch(&permissions, &on, &admin_token).await;
    assert_eq!(status, StatusCode::OK, "{now}");
    assert_eq!(now["may_link"], true, "{now}");
    let (status, _) = app.get("/api/v1/catalog", &friend_token).await;
    assert_eq!(status, StatusCode::OK);

    // A Jellyfin Policy edit leaves it alone.
    let edit = json!({"IsAdministrator": false, "EnableAllFolders": true});
    app.post(&format!("/Users/{friend}/Policy"), &edit, &admin_token)
        .await;
    let (_, mine) = app.get(&permissions, &friend_token).await;
    assert_eq!(mine["may_link"], true, "{mine}");

    // Another user's are the admin's business only.
    let (_, other_token) = app.create_user("other", false).await;
    let (status, _) = app.get(&permissions, &other_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .patch(
            &format!("/api/v1/users/{}/permissions", Uuid::new_v4()),
            &on,
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    app.patch(&permissions, &json!({"may_link": false}), &admin_token)
        .await;
    let (status, _) = app.get("/api/v1/catalog", &friend_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// `GET /api/v1/catalog`: what changed in the libraries the user can see
/// since a cursor, as ids (the details come from `/Items?Ids=`), with
/// removals. Only this server's own items: never a copy it holds from another
/// link.
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
        "EnabledFolders": [movies, shows]
    });
    let (status, _) = app
        .post(&format!("/Users/{friend}/Policy"), &policy, &admin_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .patch(
            &format!("/api/v1/users/{friend}/permissions"),
            &json!({"may_link": true}),
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Everything, from the start.
    let (status, first) = app.get("/api/v1/catalog", &token).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let mut seen = ids(&first, "items");
    seen.sort();
    let mut want = vec![film, other, series];
    want.sort();
    assert_eq!(seen, want, "{first}");
    let entry = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"].as_str() == Some(&series.to_string()))
        .cloned()
        .unwrap();
    assert_eq!(entry["type"], "Series", "{first}");
    assert_eq!(entry["library_id"], json!(shows), "{first}");
    assert_eq!(first["more"], false, "{first}");
    assert!(first["cursor"].is_string(), "{first}");

    // Nothing changed: nothing comes.
    let (_, quiet) = app.get(&catalog(&first["cursor"]), &token).await;
    assert!(ids(&quiet, "items").is_empty(), "{quiet}");
    assert!(ids(&quiet, "removed").is_empty(), "{quiet}");

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
    let (_, next) = app.get(&catalog(&quiet["cursor"]), &token).await;
    let mut changed = ids(&next, "items");
    changed.sort();
    let mut want = vec![film, other];
    want.sort();
    assert_eq!(changed, want, "{next}");
    assert_eq!(ids(&next, "removed"), vec![series], "{next}");

    // Paged: a short page says there is more and where to go on from. The
    // keys stay as sent (the Jellyfin routes' camelCase fix-up is not applied).
    let (_, page) = app.get("/api/v1/catalog?limit=1", &token).await;
    assert_eq!(ids(&page, "items").len(), 1, "{page}");
    assert_eq!(page["more"], true, "{page}");
    let after = page["next"].as_str().unwrap_or_else(|| panic!("{page}"));
    let (_, rest) = app
        .get(&format!("/api/v1/catalog?limit=10&after={after}"), &token)
        .await;
    let mut all = ids(&page, "items");
    all.extend(ids(&rest, "items"));
    all.sort();
    let mut want = vec![film, other];
    want.sort();
    assert_eq!(all, want, "{rest}");
    assert_eq!(rest["more"], false, "{rest}");

    let (status, _) = app.get("/api/v1/catalog?since=nonsense", &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app.get("/api/v1/catalog", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// Rustyfin's own routes are written in lower case but match in any case
/// (owner, 2026-11-19), like Jellyfin's ASP.NET routes; what a route passes
/// on (a relayed path, an id) keeps its case.
#[tokio::test]
async fn own_routes_match_in_any_case() {
    let app = common::TestApp::new().await;
    let (admin, admin_token) = app.create_user("admin", true).await;
    let (status, list) = app.get("/API/V1/Links", &admin_token).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list.is_array(), "{list}");
    let (status, mine) = app
        .get(&format!("/Api/v1/Users/{admin}/Permissions"), &admin_token)
        .await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    assert_eq!(mine["may_link"], false, "{mine}");
    let upper = admin.to_string().to_uppercase();
    let (status, _) = app
        .get(&format!("/api/v1/users/{upper}/permissions"), &admin_token)
        .await;
    assert_eq!(status, StatusCode::OK, "an id in capitals is the same id");
    // A route that is not there is still not there.
    let (status, _) = app.get("/API/V1/Nothing", &admin_token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// The admin's users list marks server accounts: one call answers who may
/// link, for administrators only.
#[tokio::test]
async fn administrators_see_which_users_are_servers() {
    let app = common::TestApp::new().await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (person, person_token) = app.create_user("viewer", false).await;
    let (server, _) = app.create_user("alex-server", false).await;
    let (status, _) = app
        .patch(
            &format!("/api/v1/users/{server}/permissions"),
            &json!({"may_link": true}),
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, all) = app.get("/api/v1/users/permissions", &admin_token).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    let of = |id: uuid::Uuid| {
        all.as_array()
            .unwrap_or_else(|| panic!("{all}"))
            .iter()
            .find(|p| p["id"] == id.to_string())
            .map(|p| p["may_link"].clone())
    };
    assert_eq!(of(server), Some(json!(true)), "{all}");
    assert_eq!(of(person), Some(json!(false)), "{all}");
    let (status, _) = app.get("/API/V1/Users/Permissions", &admin_token).await;
    assert_eq!(status, StatusCode::OK, "any case");
    let (status, _) = app.get("/api/v1/users/permissions", &person_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}
