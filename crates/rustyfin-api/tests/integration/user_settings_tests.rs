//! A user's saved app settings act on the server, as in Jellyfin (owner,
//! 2026-11-19: "apply settings").

use crate::common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn ids(list: &Value) -> Vec<String> {
    let items = list["Items"].as_array().or(list.as_array());
    items
        .unwrap_or_else(|| panic!("{list}"))
        .iter()
        .map(|i| i["Id"].as_str().unwrap().replace('-', ""))
        .collect()
}

fn simple(id: Uuid) -> String {
    id.simple().to_string()
}

async fn save(app: &common::TestApp, uid: Uuid, token: &str, settings: Value) {
    let (status, body) = app
        .post(&format!("/Users/{uid}/Configuration"), &settings, token)
        .await;
    assert!(status.is_success(), "{status}: {body}");
}

#[tokio::test]
async fn library_order_and_hidden_libraries_are_kept() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let films = app.create_library("Films", "movies").await;
    let shows = app.create_library("Shows", "tvshows").await;
    let music = app.create_library("Music", "music").await;
    save(
        &app,
        uid,
        &token,
        json!({
            "OrderedViews": [simple(music), films.to_string()],
            "MyMediaExcludes": [simple(shows)],
        }),
    )
    .await;
    let (status, views) = app.get("/UserViews", &token).await;
    assert_eq!(status, StatusCode::OK, "{views}");
    assert_eq!(ids(&views), vec![simple(music), simple(films)], "{views}");
    let (_, all) = app.get("/UserViews?IncludeHidden=true", &token).await;
    assert_eq!(
        ids(&all),
        vec![simple(music), simple(films), simple(shows)],
        "the app's settings page lists every library: {all}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn latest_leaves_out_excluded_libraries_and_played_titles() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let films = app.create_library("Films", "movies").await;
    let docs = app.create_library("Docs", "movies").await;
    let heat = app.create_media_item("Heat", "Movie", films, None).await;
    let alien = app.create_media_item("Alien", "Movie", films, None).await;
    let earth = app.create_media_item("Earth", "Movie", docs, None).await;
    let names = |list: &Value| -> Vec<String> {
        let mut n: Vec<String> = list
            .as_array()
            .unwrap_or_else(|| panic!("{list}"))
            .iter()
            .map(|i| i["Name"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    };
    let (status, latest) = app.get("/Items/Latest?GroupItems=false", &token).await;
    assert_eq!(status, StatusCode::OK, "{latest}");
    assert_eq!(names(&latest), vec!["Alien", "Earth", "Heat"]);

    save(
        &app,
        uid,
        &token,
        json!({"LatestItemsExcludes": [simple(docs)]}),
    )
    .await;
    let (_, latest) = app.get("/Items/Latest?GroupItems=false", &token).await;
    assert_eq!(names(&latest), vec!["Alien", "Heat"], "{latest}");
    // Asked for that library, it is shown.
    let (_, latest) = app
        .get(&format!("/Items/Latest?ParentId={docs}"), &token)
        .await;
    assert_eq!(names(&latest), vec!["Earth"], "{latest}");

    // Played titles leave Latest (Jellyfin's default), unless the user wants them.
    let (status, _) = app
        .post(&format!("/UserPlayedItems/{heat}"), &json!({}), &token)
        .await;
    assert!(status.is_success(), "{status}");
    let (_, latest) = app.get("/Items/Latest?GroupItems=false", &token).await;
    assert_eq!(names(&latest), vec!["Alien"], "{latest}");
    save(
        &app,
        uid,
        &token,
        json!({"LatestItemsExcludes": [simple(docs)], "HidePlayedInLatest": false}),
    )
    .await;
    let (_, latest) = app.get("/Items/Latest?GroupItems=false", &token).await;
    assert_eq!(names(&latest), vec!["Alien", "Heat"], "{latest}");
    let _ = (alien, earth);
    app.cleanup().await;
}

#[tokio::test]
async fn playback_starts_with_the_users_languages() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let films = app.create_library("Films", "movies").await;
    let film = app.create_media_item("Amélie", "Movie", films, None).await;
    for (index, kind, lang, default, forced) in [
        (0, "Video", "und", true, false),
        (1, "Audio", "eng", true, false),
        (2, "Audio", "fre", false, false),
        (3, "Subtitle", "eng", false, false),
        (4, "Subtitle", "fre", false, false),
    ] {
        sqlx::query(
            "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, language, \
             is_default, is_forced) VALUES ($1, $2, $3, 'x', $4, $5, $6)",
        )
        .bind(film)
        .bind(index)
        .bind(kind)
        .bind(lang)
        .bind(default)
        .bind(forced)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    let source = |info: &Value| info["MediaSources"][0].clone();

    // Nothing saved: the file's default audio, no subtitles.
    let (status, info) = app
        .get(&format!("/Items/{film}/PlaybackInfo"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(source(&info)["DefaultAudioStreamIndex"], 1, "{info}");
    assert_eq!(source(&info)["DefaultSubtitleStreamIndex"], -1, "{info}");

    save(
        &app,
        uid,
        &token,
        json!({
            "AudioLanguagePreference": "fre",
            "PlayDefaultAudioTrack": false,
            "SubtitleLanguagePreference": "eng",
            "SubtitleMode": "Always",
        }),
    )
    .await;
    let (_, info) = app
        .post(
            &format!("/Items/{film}/PlaybackInfo?UserId={uid}"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(source(&info)["DefaultAudioStreamIndex"], 2, "{info}");
    assert_eq!(source(&info)["DefaultSubtitleStreamIndex"], 3, "{info}");
    app.cleanup().await;
}
