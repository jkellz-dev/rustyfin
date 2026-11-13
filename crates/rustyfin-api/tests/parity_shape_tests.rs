//! Fields Jellyfin always sends and clients read without checking.
//!
//! Recorded from a real Jellyfin 10.10 by the Hurl parity suite. Each test names
//! the keys for one response type and asserts they are present with the right kind;
//! where Jellyfin uses a fixed default (`PlayAccess: "Full"`, empty collections,
//! `false` flags) the value is asserted too.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn has<'a>(v: &'a Value, key: &str) -> &'a Value {
    assert!(v.get(key).is_some(), "{key} missing in {v}");
    &v[key]
}

fn empty_array(v: &Value, key: &str) {
    assert_eq!(has(v, key), &json!([]), "{key}");
}

fn is_bool(v: &Value, key: &str) {
    assert!(has(v, key).is_boolean(), "{key} not a boolean: {}", v[key]);
}

fn is_string(v: &Value, key: &str) {
    assert!(has(v, key).is_string(), "{key} not a string: {}", v[key]);
}

/// A movie with a real path, a runtime and two probed streams.
async fn probed_movie(app: &common::TestApp, lib: Uuid) -> Uuid {
    let id = app.create_media_item("Probed", "Movie", lib, None).await;
    sqlx::query(
        "UPDATE media_items SET path = '/media/movies/Probed (2020)/Probed.mkv', container = 'mkv', \
         size = 123456, runtime_ticks = 54000000000, total_bitrate = 4000000 WHERE id = $1",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .expect("item media columns");
    for (idx, kind, codec, w, h, fps, ch) in [
        (
            0,
            "Video",
            "h264",
            Some(1920),
            Some(1080),
            Some(23.976f32),
            None,
        ),
        (1, "Audio", "aac", None, None, None, Some(2)),
    ] {
        sqlx::query(
            "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, \
             real_frame_rate, channels, is_default) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true)",
        )
        .bind(id)
        .bind(idx)
        .bind(kind)
        .bind(codec)
        .bind(w)
        .bind(h)
        .bind(fps)
        .bind(ch)
        .execute(&app.pool)
        .await
        .expect("insert stream");
    }
    id
}

#[tokio::test]
async fn ping_answers_a_json_string() {
    let app = common::TestApp::new().await;
    let (status, headers, body) = app.get_raw("/System/Ping", "").await;
    assert_eq!(status, StatusCode::OK);
    let ct = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    assert!(ct.starts_with("application/json"), "Content-Type was {ct}");
    let v: Value = serde_json::from_str(&body).expect("a JSON body");
    assert!(v.is_string(), "{v}");
    app.cleanup().await;
}

#[tokio::test]
async fn system_info_carries_jellyfins_fixed_fields() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;

    let (status, public) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, StatusCode::OK);
    is_string(&public, "OperatingSystem");

    let (status, info) = app.get("/System/Info", &token).await;
    assert_eq!(status, StatusCode::OK);
    empty_array(&info, "CompletedInstallations");
    is_string(&info, "WebPath");
    is_string(&info, "EncoderLocation");
    is_string(&info, "SystemArchitecture");
    let apps = has(&info, "CastReceiverApplications")
        .as_array()
        .expect("array");
    assert!(!apps.is_empty(), "{info}");
    is_string(&apps[0], "Id");
    is_string(&apps[0], "Name");
    app.cleanup().await;
}

const CONFIG_BOOLS: [&str; 5] = [
    "DisplayMissingEpisodes",
    "DisplayCollectionsView",
    "EnableLocalPassword",
    "HidePlayedInLatest",
    "PlayDefaultAudioTrack",
];
const CONFIG_ARRAYS: [&str; 4] = [
    "GroupedFolders",
    "OrderedViews",
    "LatestItemsExcludes",
    "MyMediaExcludes",
];
const POLICY_BOOLS: [&str; 11] = [
    "EnableCollectionManagement",
    "EnableSubtitleManagement",
    "EnableLyricManagement",
    "EnableRemoteControlOfOtherUsers",
    "EnableSharedDeviceControl",
    "EnableLiveTvManagement",
    "EnableLiveTvAccess",
    "EnablePlaybackRemuxing",
    "ForceRemoteSourceTranscoding",
    "EnableContentDeletion",
    "EnableMediaPlayback",
];
const POLICY_ARRAYS: [&str; 9] = [
    "BlockedTags",
    "AllowedTags",
    "AccessSchedules",
    "BlockUnratedItems",
    "EnableContentDeletionFromFolders",
    "EnabledDevices",
    "EnabledChannels",
    "BlockedMediaFolders",
    "BlockedChannels",
];

#[tokio::test]
async fn user_dto_carries_jellyfins_configuration_and_policy() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;

    for path in ["/Users/Me".to_string(), format!("/Users/{user_id}")] {
        let (status, user) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        is_string(&user, "LastActivityDate");
        let cfg = has(&user, "Configuration");
        is_string(cfg, "SubtitleLanguagePreference");
        is_string(cfg, "CastReceiverId");
        for k in CONFIG_BOOLS {
            is_bool(cfg, k);
        }
        for k in CONFIG_ARRAYS {
            empty_array(cfg, k);
        }
        let pol = has(&user, "Policy");
        for k in POLICY_BOOLS {
            is_bool(pol, k);
        }
        for k in POLICY_ARRAYS {
            empty_array(pol, k);
        }
        // Design boundaries: no LiveTV, no SyncPlay.
        assert_eq!(pol["EnableLiveTvAccess"], false, "{path}");
        assert_eq!(pol["SyncPlayAccess"], "None", "{path}");
    }
    app.cleanup().await;
}

const ITEM_EMPTY_ARRAYS: [&str; 10] = [
    "Genres",
    "Tags",
    "Taglines",
    "Studios",
    "People",
    "GenreItems",
    "ExternalUrls",
    "RemoteTrailers",
    "ProductionLocations",
    "LockedFields",
];

fn assert_item_base(item: &Value) {
    for k in ITEM_EMPTY_ARRAYS {
        empty_array(item, k);
    }
    assert_eq!(has(item, "ProviderIds"), &json!({}));
    assert_eq!(has(item, "LockData"), false);
    assert_eq!(has(item, "LocalTrailerCount"), 0);
    assert_eq!(has(item, "SpecialFeatureCount"), 0);
    assert_eq!(has(item, "PlayAccess"), "Full");
    assert_eq!(has(item, "EnableMediaSourceDisplay"), true);
    is_string(item, "DisplayPreferencesId");
    assert_eq!(has(item, "LocationType"), "FileSystem");
}

#[tokio::test]
async fn every_item_carries_jellyfins_default_fields() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = probed_movie(&app, lib).await;

    let (_, list) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_item_base(&list["Items"][0]);

    let (_, single) = app.get(&format!("/Items/{id}"), &token).await;
    assert_item_base(&single);
    is_string(&single, "Path");
    empty_array(&single, "Chapters");
    assert_eq!(has(&single, "Trickplay"), &json!({}));

    // Collection folders are items too.
    for path in [
        "/UserViews".to_string(),
        format!("/Users/{user_id}/Views"),
        "/Library/MediaFolders".to_string(),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        let folder = &body["Items"][0];
        assert_item_base(folder);
        assert_eq!(has(folder, "MediaType"), "Unknown", "{path}");
        assert_eq!(has(folder, "ImageTags"), &json!({}), "{path}");
        empty_array(folder, "BackdropImageTags");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn media_sources_and_streams_carry_jellyfins_fields() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let id = probed_movie(&app, lib).await;

    let (status, single) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, pb) = app
        .post(
            &format!("/Items/{id}/PlaybackInfo?UserId={user_id}"),
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{pb}");

    for (label, body) in [("item", &single), ("playbackinfo", &pb)] {
        let src = &has(body, "MediaSources")[0];
        assert!(src.is_object(), "{label}: {body}");
        is_string(src, "Path");
        assert_eq!(has(src, "RunTimeTicks"), 54_000_000_000_i64, "{label}");
        for k in [
            "IgnoreDts",
            "IgnoreIndex",
            "GenPtsInput",
            "IsInfiniteStream",
            "UseMostCompatibleTranscodingProfile",
            "RequiresOpening",
            "RequiresClosing",
            "RequiresLooping",
            "SupportsProbing",
            "HasSegments",
        ] {
            is_bool(src, k);
        }
        assert_eq!(has(src, "VideoType"), "VideoFile", "{label}");
        empty_array(src, "MediaAttachments");
        empty_array(src, "Formats");
        assert_eq!(has(src, "RequiredHttpHeaders"), &json!({}), "{label}");
        is_string(src, "TranscodingSubProtocol");
        assert_eq!(has(src, "DefaultAudioStreamIndex"), 1, "{label}");

        let streams = has(src, "MediaStreams").as_array().expect("streams");
        let video = streams
            .iter()
            .find(|s| s["Type"] == "Video")
            .expect("video stream");
        let audio = streams
            .iter()
            .find(|s| s["Type"] == "Audio")
            .expect("audio stream");
        for s in [video, audio] {
            is_string(s, "DisplayTitle");
            is_bool(s, "IsInterlaced");
            is_bool(s, "IsAVC");
            is_bool(s, "IsHearingImpaired");
            is_bool(s, "IsTextSubtitleStream");
            is_bool(s, "SupportsExternalStream");
            is_bool(s, "IsAnamorphic");
            assert_eq!(has(s, "AudioSpatialFormat"), "None", "{label}");
        }
        assert_eq!(video["IsAVC"], true, "{label}");
        assert_eq!(video["VideoRange"], "SDR", "{label}");
        assert_eq!(video["VideoRangeType"], "SDR", "{label}");
        assert!(video["AverageFrameRate"].is_number(), "{label}: {video}");
        assert!(video["ReferenceFrameRate"].is_number(), "{label}: {video}");
        assert_eq!(audio["IsAVC"], false, "{label}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn user_scoped_latest_route_exists() {
    let app = common::TestApp::new().await;
    let (user_id, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    probed_movie(&app, lib).await;

    let (status, body) = app
        .get(&format!("/Users/{user_id}/Items/Latest"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.is_array(), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn folders_report_how_many_items_are_unplayed() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", lib, None).await;
    let season = app
        .create_media_item("S1", "Season", lib, Some(series))
        .await;
    let e1 = app
        .create_media_item("E1", "Episode", lib, Some(season))
        .await;
    app.create_media_item("E2", "Episode", lib, Some(season))
        .await;
    app.create_media_item("E3", "Episode", lib, Some(season))
        .await;
    let (status, _) = app
        .post(&format!("/UserPlayedItems/{e1}"), &json!({}), &token)
        .await;
    assert!(status.is_success());

    for (id, label) in [(series, "series"), (season, "season")] {
        let (_, item) = app.get(&format!("/Items/{id}"), &token).await;
        assert_eq!(
            item["UserData"]["UnplayedItemCount"], 2,
            "{label}: {}",
            item["UserData"]
        );
    }
    let (_, list) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Series", &token)
        .await;
    assert_eq!(
        list["Items"][0]["UserData"]["UnplayedItemCount"], 2,
        "{list}"
    );
    // An episode is not a folder: no count.
    let (_, ep) = app.get(&format!("/Items/{e1}"), &token).await;
    assert!(
        ep["UserData"]
            .get("UnplayedItemCount")
            .is_none_or(Value::is_null),
        "{ep}"
    );
    app.cleanup().await;
}
