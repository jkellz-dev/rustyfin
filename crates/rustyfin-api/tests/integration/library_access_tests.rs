//! A user limited to some libraries must not reach items in the others.
//!
//! An administrator can restrict a user to a set of libraries
//! (`POST /Users/{id}/Policy`). The item list routes honour that, but every route
//! that takes an item id has to as well, including the ones that stream the file.
//! Jellyfin answers 404 for an item the user may not see, so that is what is
//! asserted: a 403 would confirm the item exists.
//!
//! Each route is asserted both ways. The visible item (the control) must not be a
//! 404, otherwise a route that is simply broken would look like it enforces access.
use crate::common;

use axum::http::StatusCode;
use rustyfin_db::repo::{media_items, user_library_access, users};
use uuid::Uuid;

struct World {
    app: common::TestApp,
    token: String,
    user_id: Uuid,
    /// An item in the library the restricted user may see.
    visible: Uuid,
    /// An item in the library the restricted user may not see.
    hidden: Uuid,
    hidden_series: Uuid,
    files: Vec<std::path::PathBuf>,
}

/// A real file on disk, so the streaming routes have something to serve.
fn media_file(tag: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("rustyfin-access-{tag}-{}.mkv", Uuid::new_v4()));
    std::fs::write(&path, vec![0u8; 4096]).expect("write test media file");
    path
}

async fn insert_item(
    app: &common::TestApp,
    name: &str,
    item_type: &str,
    library: Uuid,
    parent: Option<Uuid>,
    path: Option<&std::path::Path>,
) -> Uuid {
    let id = app
        .create_media_item(name, item_type, library, parent)
        .await;
    if let Some(path) = path {
        sqlx::query(
            "UPDATE media_items SET path = $1, size = 4096, container = 'mkv' WHERE id = $2",
        )
        .bind(path.to_string_lossy().to_string())
        .bind(id)
        .execute(&app.pool)
        .await
        .expect("set item path");
    }
    id
}

async fn world() -> World {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("restricted", false).await;

    let lib_ok = app.create_library("Allowed", "movies").await;
    let lib_no = app.create_library("Hidden", "movies").await;
    let shows_no = app.create_library("Hidden Shows", "tvshows").await;

    let file_ok = media_file("ok");
    let file_no = media_file("no");
    let visible = insert_item(&app, "Visible", "Movie", lib_ok, None, Some(&file_ok)).await;
    let hidden = insert_item(&app, "Hidden", "Movie", lib_no, None, Some(&file_no)).await;
    let hidden_series = insert_item(&app, "Hidden Show", "Series", shows_no, None, None).await;
    let hidden_season = insert_item(
        &app,
        "Hidden Season",
        "Season",
        shows_no,
        Some(hidden_series),
        None,
    )
    .await;
    insert_item(
        &app,
        "Hidden Episode",
        "Episode",
        shows_no,
        Some(hidden_season),
        Some(&file_no),
    )
    .await;

    // Restrict the user to the one library.
    users::update_policy(&app.pool, user_id, false, false)
        .await
        .expect("restrict user");
    user_library_access::set_enabled_libraries(&app.pool, user_id, &[lib_ok])
        .await
        .expect("grant one library");

    World {
        app,
        token,
        user_id,
        visible,
        hidden,
        hidden_series,
        files: vec![file_ok, file_no],
    }
}

impl World {
    async fn done(self) {
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
        self.app.cleanup().await;
    }
}

/// Per-item GET routes, with `{id}` standing for the item.
const ITEM_GET_ROUTES: [&str; 14] = [
    "/Items/{id}",
    "/Users/{user}/Items/{id}",
    "/Items/{id}/PlaybackInfo",
    "/Videos/{id}/stream",
    "/Videos/{id}/stream.mkv",
    "/Audio/{id}/stream",
    "/Items/{id}/Similar",
    "/Items/{id}/Ancestors",
    "/Items/{id}/ThemeSongs",
    "/Items/{id}/ThemeVideos",
    "/Items/{id}/ThemeMedia",
    "/Items/{id}/Images",
    "/Items/{id}/Images/Primary",
    "/Videos/{id}/Subtitles/0/Stream.vtt",
];

fn fill(route: &str, w: &World, id: Uuid) -> String {
    route
        .replace("{id}", &id.to_string())
        .replace("{user}", &w.user_id.to_string())
}

/// Every route must hide the other library's item.
#[tokio::test]
async fn item_routes_answer_404_for_an_item_in_a_hidden_library() {
    let w = world().await;
    let mut leaks = Vec::new();

    for route in ITEM_GET_ROUTES {
        let (status, _) = w.app.get(&fill(route, &w, w.hidden), &w.token).await;
        if status != StatusCode::NOT_FOUND {
            leaks.push(format!("GET {route} -> {status}"));
        }
    }
    for route in ["/UserPlayedItems/{id}", "/UserFavoriteItems/{id}"] {
        let (status, _) = w
            .app
            .post(&fill(route, &w, w.hidden), &serde_json::json!({}), &w.token)
            .await;
        if status != StatusCode::NOT_FOUND {
            leaks.push(format!("POST {route} -> {status}"));
        }
    }

    assert!(
        leaks.is_empty(),
        "routes that did not hide an item in a library the user cannot access:\n  {}",
        leaks.join("\n  ")
    );
    w.done().await;
}

/// The same routes work for an item in a library the user may see.
#[tokio::test]
async fn item_routes_still_serve_an_item_in_an_allowed_library() {
    let w = world().await;

    // Routes that legitimately have nothing to show for a bare movie (images, a
    // subtitle stream that does not exist) may be 404 for it too, so they are not
    // controls; the rest must be reachable.
    for route in [
        "/Items/{id}",
        "/Users/{user}/Items/{id}",
        "/Items/{id}/PlaybackInfo",
        "/Videos/{id}/stream",
        "/Items/{id}/Similar",
        "/Items/{id}/Ancestors",
        "/Items/{id}/ThemeSongs",
    ] {
        let (status, _) = w.app.get(&fill(route, &w, w.visible), &w.token).await;
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "control: GET {route} on the visible item must not be hidden"
        );
        assert!(
            status.is_success(),
            "control: GET {route} on the visible item gave {status}"
        );
    }
    w.done().await;
}

/// Lists must not contain the hidden items either.
#[tokio::test]
async fn lists_do_not_contain_items_from_a_hidden_library() {
    let w = world().await;
    // Answers write ids as 32 hex digits; look for either form.
    let hidden = [w.hidden.to_string(), w.hidden.simple().to_string()];
    let mut leaks = Vec::new();

    for route in [
        "/Items?Recursive=true",
        "/Items?Recursive=true&IncludeItemTypes=Movie",
        "/Items/Latest",
        "/Items/Suggestions",
        "/Search/Hints?searchTerm=Hidden",
        "/Search/Hints?SearchTerm=Hidden",
        "/Movies/Recommendations",
        "/Items/{id}/Similar",
        "/Users/{user}/Items?Recursive=true",
        "/Users/{user}/Suggestions",
    ] {
        let (status, body) = w.app.get(&fill(route, &w, w.visible), &w.token).await;
        if !status.is_success() {
            continue; // a route that does not exist for this shape is not a leak
        }
        let text = body.to_string();
        if hidden.iter().any(|h| text.contains(h.as_str())) {
            leaks.push(format!("GET {route} lists the hidden item"));
        }
    }

    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    w.done().await;
}

/// A hidden series must not be browsable through the TV routes.
#[tokio::test]
async fn show_routes_hide_a_series_in_a_hidden_library() {
    let w = world().await;
    let mut leaks = Vec::new();

    for route in [
        "/Shows/{series}/Seasons",
        "/Shows/{series}/Episodes",
        "/Items/{series}",
    ] {
        let path = route.replace("{series}", &w.hidden_series.to_string());
        let (status, _) = w.app.get(&path, &w.token).await;
        if status != StatusCode::NOT_FOUND {
            leaks.push(format!("GET {route} -> {status}"));
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    w.done().await;
}

/// An unrestricted user is unaffected.
#[tokio::test]
async fn an_unrestricted_user_reaches_every_library() {
    let w = world().await;
    let (_, token) = w.app.create_user("everyone", false).await;

    let (status, _) = w.app.get(&format!("/Items/{}", w.hidden), &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = w
        .app
        .get(&format!("/Videos/{}/stream", w.hidden), &token)
        .await;
    assert!(status.is_success(), "stream gave {status}");
    let _ = media_items::find_by_id(&w.app.pool, w.hidden).await;
    w.done().await;
}
