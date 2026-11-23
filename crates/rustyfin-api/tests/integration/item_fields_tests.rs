//! Fields Jellyfin sends that Rustyfin left out (parity catalog,
//! 2026-11-22): on the root folder, a season, an artist, the ancestors of an
//! episode or a track, audio stream labels, Filters2, suggestions, and the people of
//! listed titles.

use crate::common;

use axum::http::StatusCode;
use rustyfin_db::repo::{media_streams, people};
use uuid::Uuid;

async fn set(app: &common::TestApp, sql: &'static str, id: Uuid, other: Option<Uuid>) {
    let q = sqlx::query(sql).bind(id);
    let q = match other {
        Some(o) => q.bind(o),
        None => q,
    };
    q.execute(&app.pool).await.unwrap();
}

async fn audio_stream(app: &common::TestApp, item: Uuid) {
    media_streams::insert(
        &app.pool,
        &media_streams::NewMediaStream {
            item_id: item,
            stream_index: 0,
            stream_type: "Audio".to_string(),
            codec: Some("mp3".to_string()),
            channels: Some(2),
            sample_rate: Some(44100),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn the_root_folder_has_its_dates_and_user_data() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    app.create_library("Movies", "movies").await;
    for path in [
        "/Items/Root".to_string(),
        format!("/Users/{user}/Items/Root"),
    ] {
        let (status, root) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(root["Etag"].is_string(), "{path}: {root}");
        assert!(root["DateCreated"].is_string(), "{path}: {root}");
        assert!(root["UserData"]["Key"].is_string(), "{path}: {root}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn shows_and_music_carry_their_parents_and_totals() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tv = app.create_library("Shows", "tvshows").await;
    let music = app.create_library("Music", "music").await;
    let show = app.create_media_item("Show", "Series", tv, None).await;
    let season = app
        .create_media_item("Season 1", "Season", tv, Some(show))
        .await;
    let episode = app
        .create_media_item("Pilot", "Episode", tv, Some(season))
        .await;
    set(
        &app,
        "UPDATE media_items SET series_id = $2 WHERE id = $1",
        episode,
        Some(show),
    )
    .await;
    let artist = app
        .create_media_item("Band", "MusicArtist", music, None)
        .await;
    let album = app
        .create_media_item("Record", "MusicAlbum", music, Some(artist))
        .await;
    let track = app
        .create_media_item("Song", "Audio", music, Some(album))
        .await;
    set(
        &app,
        "UPDATE media_items SET runtime_ticks = 600000000, artists = ARRAY['Band'], \
         season_id = parent_id, series_id = $2, series_name = 'Band' WHERE id = $1",
        track,
        Some(artist),
    )
    .await;
    audio_stream(&app, track).await;

    let (_, s) = app.get(&format!("/Items/{season}"), &token).await;
    assert_eq!(s["SeriesName"], "Show", "{s}");
    assert_eq!(s["SeriesId"], show.simple().to_string(), "{s}");

    let (_, a) = app.get(&format!("/Items/{artist}"), &token).await;
    assert_eq!(a["SongCount"], 1, "{a}");
    assert_eq!(a["AlbumCount"], 1, "{a}");
    assert_eq!(a["MusicVideoCount"], 0, "{a}");
    assert_eq!(a["CumulativeRunTimeTicks"], 600000000, "{a}");

    let (status, up) = app
        .get(&format!("/Items/{episode}/Ancestors"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(up[0]["Type"], "Season", "{up}");
    assert_eq!(up[0]["SeriesName"], "Show", "{up}");
    assert_eq!(up[0]["SeriesId"], show.simple().to_string(), "{up}");

    let (_, up) = app.get(&format!("/Items/{track}/Ancestors"), &token).await;
    let record = &up[0];
    assert_eq!(record["Type"], "MusicAlbum", "{up}");
    assert_eq!(record["AlbumArtist"], "Band", "{record}");
    assert!(record["AlbumArtists"][0]["Id"].is_string(), "{record}");
    assert!(record["ArtistItems"].is_array(), "{record}");
    assert_eq!(record["CumulativeRunTimeTicks"], 600000000, "{record}");
    assert_eq!(record["RunTimeTicks"], 600000000, "{record}");

    let (_, t) = app.get(&format!("/Items/{track}"), &token).await;
    let stream = &t["MediaStreams"][0];
    assert_eq!(stream["LocalizedDefault"], "Default", "{stream}");
    assert_eq!(stream["LocalizedExternal"], "External", "{stream}");
    app.cleanup().await;
}

#[tokio::test]
async fn lists_carry_what_jellyfin_puts_in_them() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let show = app.create_media_item("Show", "Series", lib, None).await;
    audio_stream(&app, film).await;
    let actor = people::find_or_create(&app.pool, "Some Actor", "Actor", None, None)
        .await
        .unwrap();
    people::link_to_item(&app.pool, film, actor.id, Some("Lead"), Some(0))
        .await
        .unwrap();

    let (_, f) = app
        .get(&format!("/Items/Filters2?UserId={user}"), &token)
        .await;
    assert!(f["Tags"].is_array(), "{f}");

    let (_, s) = app
        .get(
            &format!("/Items/Suggestions?userId={user}&type=Movie"),
            &token,
        )
        .await;
    assert!(s["Items"][0]["MediaSources"].is_array(), "{s}");

    let (_, list) = app
        .get(
            "/Items?Recursive=true&IncludeItemTypes=Movie&Fields=People",
            &token,
        )
        .await;
    let cast = &list["Items"][0]["People"];
    assert_eq!(cast[0]["Name"], "Some Actor", "{list}");
    assert_eq!(cast[0]["Role"], "Lead", "{list}");
    assert_eq!(cast[0]["Type"], "Actor", "{list}");

    let (_, series) = app.get(&format!("/Items/{show}"), &token).await;
    assert!(series["AirDays"].is_array(), "{series}");
    app.cleanup().await;
}
