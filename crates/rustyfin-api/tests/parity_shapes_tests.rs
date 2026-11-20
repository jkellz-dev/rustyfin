//! Shapes the parity suite found missing against Jellyfin 10.10.7 once it
//! started holding top-level lists to their first element (2026-11-16):
//! Ancestors stopped at the title, Latest lacked counts and music fields,
//! sessions lacked PlayState and Capabilities, cultures and countries spelt
//! `ISO` as `Iso`, and libraries lacked most of their options.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn folder(app: &common::TestApp, id: Uuid) {
    sqlx::query("UPDATE media_items SET is_folder = true WHERE id = $1")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();
}

fn types(body: &Value) -> Vec<String> {
    body.as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|i| i["Type"].as_str().unwrap().to_string())
        .collect()
}

/// Ids are compared as JSON renders them, whatever the hyphenation.
fn same_id(v: &Value, id: Uuid) -> bool {
    v.as_str()
        .is_some_and(|s| s.replace('-', "") == id.simple().to_string())
}

#[tokio::test]
async fn ancestors_end_at_the_library_and_the_root_folder() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let movies = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", movies, None).await;
    let tv = app.create_library("Shows", "tvshows").await;
    let show = app.create_media_item("Show", "Series", tv, None).await;
    let season = app
        .create_media_item("Season 1", "Season", tv, Some(show))
        .await;
    let ep = app
        .create_media_item("Pilot", "Episode", tv, Some(season))
        .await;

    let (status, body) = app.get(&format!("/Items/{film}/Ancestors"), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        types(&body),
        ["CollectionFolder", "UserRootFolder"],
        "{body}"
    );
    assert!(same_id(&body[0]["Id"], movies), "{body}");

    let (_, body) = app.get(&format!("/Items/{ep}/Ancestors"), &token).await;
    assert_eq!(
        types(&body),
        ["Season", "Series", "CollectionFolder", "UserRootFolder"],
        "{body}"
    );
}

#[tokio::test]
async fn latest_carries_counts_and_music_fields() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Artist", "MusicArtist", music, None)
        .await;
    folder(&app, artist).await;
    let album = app
        .create_media_item("Album", "MusicAlbum", music, Some(artist))
        .await;
    folder(&app, album).await;
    for n in ["One", "Two"] {
        let t = app.create_media_item(n, "Audio", music, Some(album)).await;
        sqlx::query("UPDATE media_items SET runtime_ticks = 1000000000 WHERE id = $1")
            .bind(t)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    let movies = app.create_library("Movies", "movies").await;
    app.create_media_item("Film", "Movie", movies, None).await;

    let (status, body) = app
        .get(
            &format!("/Users/{uid}/Items/Latest?IncludeItemTypes=Audio"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let a = &body[0];
    assert_eq!(a["Type"], "MusicAlbum", "{body}");
    assert_eq!(a["ChildCount"], 2, "{a}");
    assert_eq!(a["RunTimeTicks"], 2_000_000_000_i64, "{a}");
    assert!(a["Artists"].is_array(), "{a}");
    assert!(a["ArtistItems"].is_array(), "{a}");

    let (_, body) = app
        .get(
            &format!("/Users/{uid}/Items/Latest?ParentId={movies}"),
            &token,
        )
        .await;
    assert_eq!(body[0]["Type"], "Movie", "{body}");
    assert_eq!(body[0]["ChildCount"], 0, "{body}");
}

#[tokio::test]
async fn sessions_always_carry_play_state_and_capabilities() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, _) = app.post("/Sessions/Playing/Ping", &json!({}), &token).await;
    assert!(status.is_success(), "{status}");
    let (status, body) = app.get("/Sessions", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let s = &body[0];
    assert_eq!(s["PlayState"]["CanSeek"], false, "{s}");
    assert_eq!(s["PlayState"]["IsPaused"], false, "{s}");
    assert_eq!(s["PlayState"]["IsMuted"], false, "{s}");
    assert_eq!(s["PlayState"]["RepeatMode"], "RepeatNone", "{s}");
    assert_eq!(s["PlayState"]["PlaybackOrder"], "Default", "{s}");
    assert!(s["Capabilities"]["PlayableMediaTypes"].is_array(), "{s}");
    assert!(s["Capabilities"]["SupportedCommands"].is_array(), "{s}");
    assert!(
        s["Capabilities"]["SupportsMediaControl"].is_boolean(),
        "{s}"
    );
    assert!(
        s["Capabilities"]["SupportsPersistentIdentifier"].is_boolean(),
        "{s}"
    );
    assert_eq!(s["AdditionalUsers"], json!([]), "{s}");
    assert_eq!(s["NowPlayingQueue"], json!([]), "{s}");
    assert_eq!(s["NowPlayingQueueFullItems"], json!([]), "{s}");
    assert_eq!(s["HasCustomDeviceName"], false, "{s}");
    assert!(s["LastPlaybackCheckIn"].is_string(), "{s}");
}

#[tokio::test]
async fn cultures_and_countries_spell_iso_as_jellyfin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (_, body) = app.get("/Localization/Cultures", &token).await;
    let c = &body[0];
    assert!(c["TwoLetterISOLanguageName"].is_string(), "{c}");
    assert!(c["ThreeLetterISOLanguageName"].is_string(), "{c}");
    assert_eq!(
        c["ThreeLetterISOLanguageNames"][0], c["ThreeLetterISOLanguageName"],
        "{c}"
    );
    let (_, body) = app.get("/Localization/Countries", &token).await;
    assert!(body[0]["TwoLetterISORegionName"].is_string(), "{body}");
    assert!(body[0]["ThreeLetterISORegionName"].is_string(), "{body}");
}

#[tokio::test]
async fn libraries_carry_jellyfins_options() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    app.create_library("Shows", "tvshows").await;
    let (status, body) = app.get("/Library/VirtualFolders", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let f = &body[0];
    assert_eq!(f["PrimaryImageItemId"], f["ItemId"], "{f}");
    let o = &f["LibraryOptions"];
    assert_eq!(o["PathInfos"][0]["Path"], "/tmp/test", "{o}");
    for key in [
        "EnableLUFSScan",
        "EnableTrickplayImageExtraction",
        "ExtractTrickplayImagesDuringLibraryScan",
        "EnableAutomaticSeriesGrouping",
        "EnableEmbeddedTitles",
        "EnableEmbeddedExtrasTitles",
        "EnableEmbeddedEpisodeInfos",
        "SkipSubtitlesIfEmbeddedSubtitlesPresent",
        "SkipSubtitlesIfAudioTrackMatches",
        "RequirePerfectSubtitleMatch",
        "SaveSubtitlesWithMedia",
        "SaveLyricsWithMedia",
        "SaveTrickplayWithMedia",
        "PreferNonstandardArtistsTag",
        "UseCustomTagDelimiters",
    ] {
        assert!(o[key].is_boolean(), "{key}: {o}");
    }
    for key in [
        "DisabledLocalMetadataReaders",
        "DisabledSubtitleFetchers",
        "SubtitleFetcherOrder",
        "DisabledMediaSegmentProviders",
        "MediaSegmentProvideOrder",
        "DisabledLyricFetchers",
        "LyricFetcherOrder",
        "CustomTagDelimiters",
        "DelimiterWhitelist",
        "TypeOptions",
    ] {
        assert!(o[key].is_array(), "{key}: {o}");
    }
    assert_eq!(o["SeasonZeroDisplayName"], "Specials", "{o}");
    assert_eq!(o["AutomaticRefreshIntervalDays"], 0, "{o}");
    assert_eq!(o["AllowEmbeddedSubtitles"], "AllowAll", "{o}");
}

/// Nightly parity (browse.hurl:189): Jellyfin's ancestors carry the user's
/// data and child counts, the library first among them.
#[tokio::test]
async fn ancestors_carry_user_data_and_child_counts() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let (status, body) = app.get(&format!("/Items/{film}/Ancestors"), &token).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let first = &body[0];
    assert_eq!(first["Type"], "CollectionFolder", "{body}");
    assert!(first["UserData"]["Key"].is_string(), "{first}");
    assert!(first["UserData"]["Played"].is_boolean(), "{first}");
    assert_eq!(first["ChildCount"], 1, "{first}");
    for a in body.as_array().unwrap() {
        assert!(a["UserData"].is_object(), "{a}");
        assert!(a["ChildCount"].is_number(), "{a}");
    }
    app.cleanup().await;
}
