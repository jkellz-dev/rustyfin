//! Jellyfin routes Rustyfin did not answer yet (tests/parity/ROUTES.md): the
//! older library notifications, the image path that carries its
//! options as path segments, obsolete lists Jellyfin still answers, the
//! session extras and the startup user.

use crate::common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustyfin_core::LibraryScanner;
use rustyfin_data::repo::libraries;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

/// Jellyfin's older library notifications name a title by its
/// TVDB, IMDb or TMDB id (`POST /Library/Series/Updated?tvdbId=`,
/// `/Library/Movies/Updated?imdbId=`); Jellyfin rescans that title's folder.
#[tokio::test]
async fn the_arrs_older_notifications_rescan_the_title_they_name() {
    let app = common::TestApp::new().await;
    let key = "library-key-for-tests";
    rustyfin_data::repo::api_keys::create(
        &app.pool,
        "Library tool",
        &rustyfin_auth::api_key::hash_api_key(key),
    )
    .await
    .unwrap();
    let root = std::env::temp_dir().join(format!("rf-arr-ids-{}", Uuid::new_v4().simple()));
    let film = root.join("films").join("Film (2020)");
    let season = root.join("shows").join("Show").join("Season 01");
    std::fs::create_dir_all(&film).unwrap();
    std::fs::create_dir_all(&season).unwrap();
    std::fs::write(film.join("Film (2020).mkv"), b"").unwrap();
    std::fs::write(season.join("Show S01E01.mkv"), b"").unwrap();
    let scanner = LibraryScanner::new(app.pool.clone(), None);
    for (name, kind, dir) in [("Movies", "movies", "films"), ("TV", "tvshows", "shows")] {
        let path = root.join(dir).to_string_lossy().to_string();
        let id = libraries::create(&app.pool, name, kind, std::slice::from_ref(&path))
            .await
            .unwrap()
            .id;
        scanner.scan(id, std::slice::from_ref(&path)).await.unwrap();
    }
    sqlx::query(
        "UPDATE media_items SET external_ids = CASE item_type \
         WHEN 'Movie' THEN '{\"Imdb\":\"tt0133093\",\"Tmdb\":\"603\"}'::jsonb \
         ELSE '{\"Tvdb\":\"81189\"}'::jsonb END \
         WHERE item_type IN ('Movie', 'Series')",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    // A poster and a new episode arrive, then the server is told.
    std::fs::write(film.join("poster.jpg"), b"\xFF\xD8\xFF\xE0x").unwrap();
    std::fs::write(season.join("Show S01E02.mkv"), b"").unwrap();
    let send = |uri: &str, with_key: bool| {
        let mut req = Request::builder().method("POST").uri(uri);
        if with_key {
            req = req.header("X-Api-Key", key);
        }
        app.router.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    for uri in [
        "/Library/Movies/Updated?imdbId=TT0133093",
        "/Library/Series/Added?tvdbId=81189",
        "/Library/Movies/Added?tmdbId=1",
        "/Library/Series/Updated",
    ] {
        let status = send(uri, true).await.unwrap().status();
        assert_eq!(status, StatusCode::NO_CONTENT, "{uri}");
        let status = send(uri, false).await.unwrap().status();
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} without a key");
    }

    let count = |sql: &'static str| {
        let pool = app.pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let mut done = false;
    for _ in 0..100 {
        let art = count(
            "SELECT count(*) FROM images i JOIN media_items m ON m.id = i.entity_id \
             WHERE m.item_type = 'Movie' AND i.image_type = 'Primary'",
        )
        .await;
        let episodes = count("SELECT count(*) FROM media_items WHERE item_type = 'Episode'").await;
        if art == 1 && episodes == 2 {
            done = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(done, "the film's poster and the new episode were scanned");
    std::fs::remove_dir_all(&root).ok();
    app.cleanup().await;
}

#[tokio::test]
async fn jellyfins_small_routes_answer_as_it_does() {
    let app = common::TestApp::new().await;
    let (admin, admin_token) = app.create_user("first", true).await;
    let (user, token) = app.create_user("listener", false).await;
    let lib = app.create_library("Films", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;

    // Obsolete lists Jellyfin still answers, always empty.
    for path in ["/Trailers", &format!("/Items/{film}/CriticReviews")] {
        let (status, body) = app.get(path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert_eq!(body["Items"], json!([]), "{path}");
        assert_eq!(body["TotalRecordCount"], 0, "{path}");
    }
    let (status, body) = app.get("/System/WakeOnLanInfo", &token).await;
    assert_eq!((status, body), (StatusCode::OK, json!([])));

    // The image path that carries the query's options as segments.
    let short = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary/0?maxWidth=40"),
            &token,
        )
        .await;
    let long = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary/0/x/Jpg/40/40/0/0"),
            &token,
        )
        .await;
    assert_eq!(long.0, short.0, "same answer as the query form");
    assert_eq!(long.2.is_empty(), short.2.is_empty());

    // Session extras: a system command is a general command; Rustyfin's
    // sessions carry one user, so adding or removing others changes nothing.
    let session = Uuid::new_v4();
    let (status, _) = app
        .post(
            &format!("/Sessions/{session}/System/GoHome"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .post(
            &format!("/Sessions/{session}/User/{admin}"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .delete(&format!("/Sessions/{session}/User/{admin}"), &token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // POST /Users/Configuration: the signed-in user's, or ?userId= (admin).
    let (status, _) = app
        .post(
            "/Users/Configuration",
            &json!({"SubtitleMode": "Always"}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, dto) = app.get(&format!("/Users/{user}"), &token).await;
    assert_eq!(dto["Configuration"]["SubtitleMode"], "Always", "{dto}");
    let (status, _) = app
        .post(
            &format!("/Users/Configuration?userId={admin}"),
            &json!({"SubtitleMode": "None"}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "someone else's settings");

    // Quick Connect stays off whichever verb starts it.
    let (status, _) = app.get("/QuickConnect/Initiate", &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The startup wizard's user: the first one made; no password to send.
    for path in ["/Startup/User", "/Startup/FirstUser"] {
        let (status, body) = app.get(path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(body, json!({"Name": "first", "Password": null}), "{path}");
        let (status, _) = app.get(path, &token).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path} is for admins");
    }
    app.cleanup().await;
}
