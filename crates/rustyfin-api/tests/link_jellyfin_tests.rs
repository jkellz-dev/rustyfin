//! Merged libraries, slice 1 (roadmap 16b, owner 2026-11-18: "Jellyswarrm is
//! the functionality I want... the unified interface for multiple libraries"
//! and "this should work with current Jellyfin servers").
//!
//! A friend's Jellyfin shares its "Films" library through a link user. After a
//! sync the friend's films are in the local Movies library: one the server
//! already holds becomes another copy of it (and, being better, plays by
//! default); one it lacks is listed like any other film. Its poster and its
//! bytes come through this server, so clients never talk to the friend's.
#[allow(dead_code)]
mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use rustyfin_core::LibraryScanner;
use rustyfin_core::link::{LibraryMap, LinkKind, LinkSettings};
use rustyfin_db::repo::media_items;
use serde_json::{Value, json};
use uuid::Uuid;

const FILM_BYTES: &[u8] = b"0123456789-remote-film-bytes";
const POSTER: &[u8] = b"\x89PNG-remote-poster";

/// What the fake Jellyfin serves; tests change it between syncs.
#[derive(Default)]
struct Remote {
    /// The "Films" library (lib1).
    films: Vec<Value>,
    /// The "Home Videos" library (lib2).
    videos: Vec<Value>,
    /// The "Shows" library (lib3): series, seasons and episodes.
    shows: Vec<Value>,
    /// Requests seen, as "METHOD path?query".
    seen: Vec<String>,
    /// PlaybackInfo bodies the link sent.
    playback_bodies: Vec<Value>,
}

type Shared = Arc<Mutex<Remote>>;

fn film(id: &str, name: &str, tmdb: &str, etag: &str, width: i32, height: i32) -> Value {
    json!({
        "Id": id,
        "Name": name,
        "SortName": name.to_lowercase(),
        "Type": "Movie",
        "ProductionYear": 2001,
        "Overview": format!("{name} overview"),
        "Genres": ["Drama"],
        "OfficialRating": "PG",
        "CommunityRating": 7.5,
        "RunTimeTicks": 60_000_000_000_i64,
        "PremiereDate": "2001-03-04T00:00:00.0000000Z",
        "DateCreated": "2024-05-06T07:08:09.0000000Z",
        "ProviderIds": {"Tmdb": tmdb, "Imdb": format!("tt{tmdb}")},
        "Etag": etag,
        "ImageTags": {"Primary": format!("p-{etag}")},
        "BackdropImageTags": [],
        "MediaSources": [{
            "Id": id,
            "Container": "mkv",
            "Size": FILM_BYTES.len(),
            "Bitrate": 30_000_000,
            "Name": format!("{height}p"),
            "MediaStreams": [
                {"Index": 0, "Type": "Video", "Codec": "hevc", "Width": width, "Height": height,
                 "BitRate": 28_000_000, "ColorTransfer": "smpte2084", "IsDefault": true},
                {"Index": 1, "Type": "Audio", "Codec": "eac3", "Channels": 6,
                 "Language": "eng", "IsDefault": true}
            ]
        }]
    })
}

fn authorised(headers: &HeaderMap) -> bool {
    let token = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    token("authorization").contains("Token=\"tok\"") || token("x-emby-token") == "tok"
}

async fn authenticate(State(remote): State<Shared>, body: axum::Json<Value>) -> Response {
    remote
        .lock()
        .unwrap()
        .seen
        .push("POST /Users/AuthenticateByName".into());
    if body["Username"] == "link" && body["Pw"] == "secret" {
        axum::Json(json!({"AccessToken": "tok", "User": {"Id": "u1", "Name": "link"}}))
            .into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn views(headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !authorised(&headers) || q.get("userId").map(String::as_str) != Some("u1") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(json!({"Items": [
        {"Id": "lib1", "Name": "Films", "CollectionType": "movies"},
        {"Id": "lib2", "Name": "Home Videos", "CollectionType": "homevideos"},
        {"Id": "lib3", "Name": "Shows", "CollectionType": "tvshows"}
    ], "TotalRecordCount": 3}))
    .into_response()
}

async fn items(
    State(remote): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authorised(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut r = remote.lock().unwrap();
    let mut keys: Vec<_> = q.iter().map(|(k, v)| format!("{k}={v}")).collect();
    keys.sort();
    r.seen.push(format!("GET /Items?{}", keys.join("&")));
    assert_eq!(q.get("userId").map(String::as_str), Some("u1"), "{q:?}");
    let everything: Vec<Value> = r
        .films
        .iter()
        .chain(&r.videos)
        .chain(&r.shows)
        .cloned()
        .collect();
    let (films, total): (Vec<Value>, usize) = if let Some(ids) = q.get("Ids") {
        let ids: Vec<&str> = ids.split(',').collect();
        let found: Vec<Value> = everything
            .into_iter()
            .filter(|f| ids.contains(&f["Id"].as_str().unwrap()))
            .collect();
        let n = found.len();
        (found, n)
    } else {
        let library = match q.get("ParentId").map(String::as_str) {
            Some("lib1") => &r.films,
            Some("lib2") => &r.videos,
            Some("lib3") => &r.shows,
            other => panic!("no library {other:?}"),
        };
        let types: Vec<&str> = q
            .get("IncludeItemTypes")
            .map(|t| t.split(',').collect())
            .unwrap_or_default();
        let matching: Vec<&Value> = library
            .iter()
            .filter(|f| types.contains(&f["Type"].as_str().unwrap()))
            .collect();
        let start: usize = q
            .get("StartIndex")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let limit: usize = q
            .get("Limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        let page = matching
            .iter()
            .skip(start)
            .take(limit)
            .map(|f| json!({"Id": f["Id"], "Etag": f["Etag"], "Type": f["Type"]}))
            .collect();
        (page, matching.len())
    };
    axum::Json(json!({"Items": films, "TotalRecordCount": total})).into_response()
}

async fn stream(
    State(remote): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authorised(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    remote.lock().unwrap().seen.push(format!(
        "GET /Videos/{id}/stream static={:?} source={:?}",
        q.get("static"),
        q.get("MediaSourceId")
    ));
    let len = FILM_BYTES.len();
    if let Some(range) = headers.get("range").and_then(|v| v.to_str().ok()) {
        let spec = range.trim_start_matches("bytes=");
        let (a, b) = spec.split_once('-').unwrap();
        let a: usize = a.parse().unwrap();
        let b: usize = b.parse().unwrap_or(len - 1).min(len - 1);
        return (
            StatusCode::PARTIAL_CONTENT,
            [
                ("content-range", format!("bytes {a}-{b}/{len}")),
                ("content-type", "video/x-matroska".to_string()),
                ("accept-ranges", "bytes".to_string()),
                ("etag", "\"remote-etag\"".to_string()),
            ],
            FILM_BYTES[a..=b].to_vec(),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            ("content-type", "video/x-matroska".to_string()),
            ("accept-ranges", "bytes".to_string()),
        ],
        FILM_BYTES.to_vec(),
    )
        .into_response()
}

async fn image(headers: HeaderMap, Path((id, kind)): Path<(String, String)>) -> Response {
    // Jellyfin serves images without auth; the link sends its token anyway.
    let _ = authorised(&headers);
    if id == "r100" || id == "r200" {
        assert_eq!(kind, "Primary");
        ([("content-type", "image/png")], POSTER.to_vec()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// Jellyfin's answer for a client that cannot play the file as is: an HLS
/// transcode, its URL carrying the link user's token (as Jellyfin's does).
async fn playback_info(
    State(remote): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if !authorised(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let source = body["MediaSourceId"].as_str().unwrap_or(&id).to_string();
    remote.lock().unwrap().playback_bodies.push(body);
    axum::Json(json!({
        "MediaSources": [{
            "Id": source,
            "SupportsDirectPlay": false,
            "SupportsTranscoding": true,
            "TranscodingUrl": format!(
                "/videos/{id}/master.m3u8?MediaSourceId={source}&PlaySessionId=p1&VideoCodec=h264&api_key=tok"
            ),
            "TranscodingSubProtocol": "hls",
            "TranscodingContainer": "ts"
        }],
        "PlaySessionId": "p1"
    }))
    .into_response()
}

/// HLS requests must be the link's, never carry a client's token.
fn hls_allowed(headers: &HeaderMap, q: &HashMap<String, String>) -> bool {
    authorised(headers)
        && q.iter()
            .all(|(k, v)| !k.eq_ignore_ascii_case("api_key") || v == "tok")
}

async fn master(headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !hls_allowed(&headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [("content-type", "application/vnd.apple.mpegurl")],
        "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000000\nmain.m3u8?MediaSourceId=r200&PlaySessionId=p1&api_key=tok\n",
    )
        .into_response()
}

async fn variant(headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !hls_allowed(&headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [("content-type", "application/vnd.apple.mpegurl")],
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MAP:URI=\"hls1/main/-1.mp4?runtimeTicks=0&api_key=tok\"\n#EXTINF:6.0,\nhls1/main/0.mp4?runtimeTicks=0\n#EXT-X-ENDLIST\n",
    )
        .into_response()
}

async fn segment(
    headers: HeaderMap,
    Path((_id, segment)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !hls_allowed(&headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    format!("SEGMENT {segment}").into_response()
}

/// A fake Jellyfin 10.10 on a free port; returns its base URL.
async fn fake_jellyfin(remote: Shared) -> String {
    let app = Router::new()
        .route("/Users/AuthenticateByName", post(authenticate))
        .route("/UserViews", get(views))
        .route("/Items", get(items))
        .route("/Videos/{id}/stream", get(stream))
        .route("/Items/{id}/PlaybackInfo", post(playback_info))
        .route("/videos/{id}/master.m3u8", get(master))
        .route("/videos/{id}/main.m3u8", get(variant))
        .route("/videos/{id}/hls1/main/{segment}", get(segment))
        .route("/Items/{id}/Images/{kind}", get(image))
        .route(
            "/System/Info/Public",
            get(|| async {
                axum::Json(
                    json!({"ProductName": "Jellyfin Server", "Version": "10.10.7",
                                  "Id": "remote-server-id", "ServerName": "Alex"}),
                )
            }),
        )
        .with_state(remote);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn settings(url: &str, password: &str) -> LinkSettings {
    LinkSettings {
        name: "Alex".into(),
        kind: LinkKind::Jellyfin,
        url: url.into(),
        user: "link".into(),
        password: password.into(),
        libraries: vec![LibraryMap {
            remote: "Films".into(),
            local: Some("Movies".into()),
        }],
    }
}

struct Tree(PathBuf);

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// A Movies library holding "Shared Film" (TMDB 100) at 1080p.
async fn local_library(app: &common::TestApp) -> (Tree, Uuid, Uuid) {
    let dir = std::env::temp_dir().join(format!("rustyfin-link-{}", Uuid::new_v4()));
    let file = dir.join("Shared Film (2001)/Shared Film (2001).mkv");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"local-film").unwrap();
    let root = dir.to_string_lossy().to_string();
    let lib: Uuid = sqlx::query_scalar(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Movies', 'movies', ARRAY[$1]) RETURNING id",
    )
    .bind(&root)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root])
        .await
        .expect("scan");
    let film: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE path = $1")
        .bind(file.to_string_lossy().to_string())
        .fetch_one(&app.pool)
        .await
        .unwrap();
    media_items::add_external_ids(&app.pool, film, &json!({"Tmdb": "100"}))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, is_default) \
         VALUES ($1, 0, 'Video', 'h264', 1920, 1080, true)",
    )
    .bind(film)
    .execute(&app.pool)
    .await
    .unwrap();
    (Tree(dir), lib, film)
}

fn same(v: &Value, id: Uuid) -> bool {
    v.as_str()
        .is_some_and(|s| s.replace('-', "") == id.simple().to_string())
}

fn names(list: &Value) -> Vec<String> {
    let mut n: Vec<String> = list["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{list}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

#[tokio::test]
async fn a_linked_jellyfin_library_merges_into_the_local_one() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![
        film("r100", "Shared Film", "100", "e1", 3840, 2160),
        film("r200", "Only Theirs", "200", "e1", 1920, 1080),
    ];
    let url = fake_jellyfin(remote.clone()).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "secret"))
        .await
        .unwrap();

    let report = app.links.sync(&app.pool, link).await.expect("sync");
    assert_eq!(
        (report.added, report.updated, report.removed),
        (2, 0, 0),
        "{report:?}"
    );

    // One list: the local film once, plus the friend's other film.
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    assert_eq!(list["TotalRecordCount"], 2, "{list}");

    // The shared film has both copies, the friend's 2160p first and named
    // after the friend.
    let (_, item) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    let sources = item["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{item}"));
    assert_eq!(sources.len(), 2, "{item}");
    assert!(
        sources[0]["Name"].as_str().unwrap().contains("Alex"),
        "{item}"
    );
    assert!(same(&sources[1]["Id"], local), "{item}");
    assert!(
        sources[0].get("Path").is_none_or(Value::is_null),
        "no remote paths: {item}"
    );

    // The friend's only film reads like a local one: metadata, poster.
    let theirs = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Only Theirs")
        .unwrap();
    let theirs_id = theirs["Id"].as_str().unwrap().to_string();
    let (_, detail) = app
        .get(&format!("/Users/{uid}/Items/{theirs_id}"), &token)
        .await;
    assert!(
        detail["MediaSources"][0]["Name"]
            .as_str()
            .is_some_and(|n| n.contains("Alex")),
        "a copy only the friend has is named for them too: {detail}"
    );
    assert_eq!(detail["ProductionYear"], 2001, "{detail}");
    assert_eq!(detail["Overview"], "Only Theirs overview", "{detail}");
    assert_eq!(detail["ProviderIds"]["Tmdb"], "200", "{detail}");
    assert!(detail["ImageTags"]["Primary"].is_string(), "{detail}");
    let (status, _, poster) = app
        .get_bytes(&format!("/Items/{theirs_id}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(poster, POSTER);

    // Its bytes come through this server, ranges and all.
    let (status, headers, bytes) = app
        .get_bytes_with(
            &format!("/Videos/{theirs_id}/stream?static=true"),
            &token,
            &[("Range", "bytes=2-5")],
        )
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(bytes, b"2345");
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        header("content-range"),
        format!("bytes 2-5/{}", FILM_BYTES.len())
    );

    // The local film had no poster, overview or IMDb id: it borrows the
    // friend's, and keeps what it had (its TMDB id, its name).
    let (_, mine) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    assert_eq!(mine["Overview"], "Shared Film overview", "{mine}");
    assert_eq!(mine["ProviderIds"]["Imdb"], "tt100", "{mine}");
    assert_eq!(mine["ProviderIds"]["Tmdb"], "100", "{mine}");
    assert!(mine["ImageTags"]["Primary"].is_string(), "{mine}");
    let (status, _, poster) = app
        .get_bytes(&format!("/Items/{local}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(poster, POSTER);

    // With no copy named, the shared film plays the friend's better copy.
    let (status, _, bytes) = app
        .get_bytes(&format!("/Videos/{local}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, FILM_BYTES);
    assert!(
        remote
            .lock()
            .unwrap()
            .seen
            .iter()
            .any(|s| s.starts_with("GET /Videos/r100/stream")),
        "relayed to the friend's copy"
    );

    // A linked copy plays as is; this server cannot convert what it lacks.
    let (_, info) = app
        .post(
            &format!("/Items/{theirs_id}/PlaybackInfo?UserId={uid}"),
            &json!({}),
            &token,
        )
        .await;
    let source = &info["MediaSources"][0];
    assert_eq!(source["SupportsDirectPlay"], true, "{info}");
    assert_eq!(source["SupportsTranscoding"], false, "{info}");

    app.cleanup().await;
}

#[tokio::test]
async fn a_later_sync_updates_and_removes_and_fetches_only_changes() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![
        film("r100", "Shared Film", "100", "e1", 3840, 2160),
        film("r200", "Only Theirs", "200", "e1", 1920, 1080),
        film("r300", "Third", "300", "e1", 1280, 720),
    ];
    let url = fake_jellyfin(remote.clone()).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "secret"))
        .await
        .unwrap();
    app.links.sync(&app.pool, link).await.expect("first sync");

    {
        let mut r = remote.lock().unwrap();
        r.films.retain(|f| f["Id"] != "r200");
        r.films[1] = film("r300", "Third (Remastered)", "300", "e2", 1920, 1080);
        r.seen.clear();
    }
    let report = app.links.sync(&app.pool, link).await.expect("second sync");
    assert_eq!(
        (report.added, report.updated, report.removed),
        (0, 1, 1),
        "{report:?}"
    );
    let detail_fetches: Vec<String> = remote
        .lock()
        .unwrap()
        .seen
        .iter()
        .filter(|s| s.contains("Ids="))
        .cloned()
        .collect();
    assert_eq!(detail_fetches.len(), 1, "{detail_fetches:?}");
    assert!(
        detail_fetches[0].contains("Ids=r300&"),
        "only the changed film: {detail_fetches:?}"
    );

    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(
        names(&list),
        vec!["Shared Film", "Third (Remastered)"],
        "{list}"
    );
    app.cleanup().await;
}

/// A film this server gains later takes in the friend's copy at the next sync.
#[tokio::test]
async fn a_film_gained_locally_takes_in_the_friends_copy() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (tree, lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![film("r400", "Gained", "400", "e1", 1280, 720)];
    let url = fake_jellyfin(remote).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "secret"))
        .await
        .unwrap();
    app.links.sync(&app.pool, link).await.expect("first sync");

    let file = tree.0.join("Gained (2001)/Gained (2001).mkv");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"gained").unwrap();
    std::fs::write(
        tree.0.join("Gained (2001)/.rustyfin.toml"),
        "[ids]\ntmdb = 400\n",
    )
    .unwrap();
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[tree.0.to_string_lossy().to_string()])
        .await
        .expect("rescan");
    let gained: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE path = $1")
        .bind(file.to_string_lossy().to_string())
        .fetch_one(&app.pool)
        .await
        .unwrap();
    // Merged by the scan itself: never listed twice until the next sync.
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(
        names(&list),
        vec!["Gained", "Shared Film"],
        "after the scan: {list}"
    );
    let report = app.links.sync(&app.pool, link).await.expect("second sync");
    assert_eq!(report.merged, 0, "{report:?}");

    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Gained", "Shared Film"], "{list}");
    let (_, item) = app
        .get(&format!("/Users/{uid}/Items/{gained}"), &token)
        .await;
    assert_eq!(
        item["MediaSources"].as_array().map(Vec::len),
        Some(2),
        "{item}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_wrong_password_is_reported_and_nothing_changes() {
    let app = common::TestApp::new().await;
    let (_tree, _lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![film("r200", "Only Theirs", "200", "e1", 1920, 1080)];
    let url = fake_jellyfin(remote).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "wrong"))
        .await
        .unwrap();
    let err = app.links.sync(&app.pool, link).await.unwrap_err();
    assert!(err.to_string().contains("sign in"), "{err}");
    let linked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE link_id IS NOT NULL")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(linked, 0);
    let error: Option<String> = sqlx::query_scalar("SELECT last_error FROM links WHERE id = $1")
        .bind(link)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert!(error.is_some_and(|e| e.contains("sign in")));
    app.cleanup().await;
}

/// A mapping that names a library the friend does not share, or a local
/// library that does not exist, is an error for that mapping only: the rest
/// of the link syncs, and nothing is removed while a mapping fails (its
/// films might only be out of sight for a moment).
#[tokio::test]
async fn a_bad_mapping_skips_itself_and_removes_nothing() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![film("r200", "Only Theirs", "200", "e1", 1920, 1080)];
    let url = fake_jellyfin(remote.clone()).await;
    let mut s = settings(&url, "secret");
    s.libraries.push(LibraryMap {
        remote: "Missing".into(),
        local: Some("Movies".into()),
    });
    s.libraries.push(LibraryMap {
        remote: "Films".into(),
        local: Some("Nope".into()),
    });
    let link = app.links.register(&app.pool, s).await.unwrap();
    let report = app
        .links
        .sync(&app.pool, link)
        .await
        .expect("the rest syncs");
    assert_eq!(report.added, 1, "{report:?}");
    assert_eq!(report.errors.len(), 2, "{report:?}");
    assert!(
        report.errors.iter().any(|e| e.contains("Missing")),
        "{report:?}"
    );
    assert!(
        report.errors.iter().any(|e| e.contains("Nope")),
        "{report:?}"
    );
    let error: Option<String> = sqlx::query_scalar("SELECT last_error FROM links WHERE id = $1")
        .bind(link)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert!(error.is_some_and(|e| e.contains("Missing")));
    let no_nope: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM libraries WHERE name = 'Nope'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(no_nope, 0, "a misspelt local library is not made");

    remote.lock().unwrap().films.clear();
    let report = app.links.sync(&app.pool, link).await.expect("sync again");
    assert_eq!(report.removed, 0, "{report:?}");
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    app.cleanup().await;
}

/// A Jellyfin film with two versions (two media sources) is one film here
/// with two copies, each played by its own source id.
#[tokio::test]
async fn every_version_of_a_friends_film_is_a_copy() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    let mut two = film("r500", "Two Cuts", "500", "e1", 1920, 1080);
    let mut second = two["MediaSources"][0].clone();
    second["Id"] = json!("r500b");
    second["Name"] = json!("2160p Director's Cut");
    second["MediaStreams"][0]["Width"] = json!(3840);
    second["MediaStreams"][0]["Height"] = json!(2160);
    two["MediaSources"].as_array_mut().unwrap().push(second);
    remote.lock().unwrap().films = vec![two];
    let url = fake_jellyfin(remote.clone()).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "secret"))
        .await
        .unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");

    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Shared Film", "Two Cuts"], "{list}");
    let id = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Two Cuts")
        .unwrap()["Id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, item) = app.get(&format!("/Users/{uid}/Items/{id}"), &token).await;
    let sources = item["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{item}"));
    assert_eq!(sources.len(), 2, "{item}");
    assert!(
        sources[0]["Name"]
            .as_str()
            .is_some_and(|n| n.contains("Director's Cut") && n.contains("Alex")),
        "the 2160p cut first: {item}"
    );
    let cut = sources[0]["Id"].as_str().unwrap().to_string();
    let (status, _, _) = app
        .get_bytes(
            &format!("/Videos/{id}/stream?static=true&MediaSourceId={cut}"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        remote
            .lock()
            .unwrap()
            .seen
            .iter()
            .any(|s| s.starts_with("GET /Videos/r500/stream") && s.contains("Some(\"r500b\")")),
        "the director's cut asked for by its source id: {:?}",
        remote.lock().unwrap().seen
    );

    // The friend drops the second cut: its copy goes, the film stays.
    {
        let mut r = remote.lock().unwrap();
        r.films[0]["MediaSources"].as_array_mut().unwrap().pop();
        r.films[0]["Etag"] = json!("e2");
    }
    app.links.sync(&app.pool, link).await.expect("sync again");
    let (_, item) = app.get(&format!("/Users/{uid}/Items/{id}"), &token).await;
    assert!(
        item["MediaSources"]
            .as_array()
            .is_some_and(|s| s.len() == 1),
        "{item}"
    );
    app.cleanup().await;
}

fn video(id: &str, name: &str) -> Value {
    let mut v = film(id, name, "", "e1", 1280, 720);
    v["Type"] = json!("Video");
    v["ProviderIds"] = json!({});
    v
}

/// A friend's library with no local counterpart (owner, 2026-11-18): a
/// mapping with no `local` makes a library of its own, named for the
/// friend's, with no folders. Home videos (Jellyfin `Video` items) come
/// across too.
#[tokio::test]
async fn a_friends_library_without_a_local_one_gets_its_own() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, _lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().videos = vec![video("v1", "Birthday 2019"), video("v2", "Beach")];
    let url = fake_jellyfin(remote.clone()).await;
    let mut s = settings(&url, "secret");
    s.libraries = vec![LibraryMap {
        remote: "Home Videos".into(),
        local: None,
    }];
    let link = app.links.register(&app.pool, s.clone()).await.unwrap();
    let report = app.links.sync(&app.pool, link).await.expect("sync");
    assert_eq!(report.added, 2, "{report:?}");

    let (_, views) = app.get(&format!("/Users/{uid}/Views"), &token).await;
    let view = views["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{views}"))
        .iter()
        .find(|v| v["Name"] == "Home Videos · Alex")
        .unwrap_or_else(|| panic!("a library of its own: {views}"))
        .clone();
    let shorts = view["Id"].as_str().unwrap().to_string();
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={shorts}&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Beach", "Birthday 2019"], "{list}");

    // It has no folders: a scan of it leaves the friend's videos alone.
    let id = Uuid::parse_str(&shorts).unwrap();
    LibraryScanner::new(app.pool.clone(), None)
        .scan(id, &[])
        .await
        .expect("scan");
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={shorts}&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Beach", "Birthday 2019"], "{list}");

    // The same library is used again, not a second one.
    app.links.register(&app.pool, s).await.unwrap();
    app.links.sync(&app.pool, link).await.expect("sync again");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM libraries WHERE name = 'Home Videos · Alex'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    app.cleanup().await;
}

fn series(id: &str, name: &str, tvdb: &str) -> Value {
    json!({"Id": id, "Name": name, "Type": "Series", "ProductionYear": 2020,
           "Overview": format!("{name} overview"),
           "ProviderIds": {"Tvdb": tvdb}, "Etag": "e1"})
}

fn season(id: &str, series: &str, index: i32) -> Value {
    json!({"Id": id, "Name": format!("Season {index}"), "Type": "Season",
           "IndexNumber": index, "SeriesId": series, "ParentId": series,
           "ProviderIds": {}, "Etag": "e1"})
}

fn episode(id: &str, series: &str, season: &str, s: i32, e: i32, height: i32) -> Value {
    let mut ep = film(
        id,
        &format!("Episode {s}x{e}"),
        "",
        "e1",
        height * 16 / 9,
        height,
    );
    ep["Type"] = json!("Episode");
    ep["ProviderIds"] = json!({});
    ep["IndexNumber"] = json!(e);
    ep["ParentIndexNumber"] = json!(s);
    ep["SeriesId"] = json!(series);
    ep["SeasonId"] = json!(season);
    ep["ParentId"] = json!(season);
    ep
}

/// A Shows library holding "Show" (TVDB 1) with S01E01 at 1080p.
async fn local_shows(app: &common::TestApp) -> (Tree, Uuid, Uuid, Uuid, Uuid) {
    let dir = std::env::temp_dir().join(format!("rustyfin-link-shows-{}", Uuid::new_v4()));
    let file = dir.join("Show (2020)/Season 01/Show S01E01.mkv");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"local-episode").unwrap();
    let root = dir.to_string_lossy().to_string();
    let lib: Uuid = sqlx::query_scalar(
        "INSERT INTO libraries (name, library_type, paths) VALUES ('Shows', 'tvshows', ARRAY[$1]) RETURNING id",
    )
    .bind(&root)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root])
        .await
        .expect("scan");
    let (episode, season, series): (Uuid, Uuid, Uuid) =
        sqlx::query_as("SELECT id, season_id, series_id FROM media_items WHERE path = $1")
            .bind(file.to_string_lossy().to_string())
            .fetch_one(&app.pool)
            .await
            .unwrap();
    media_items::add_external_ids(&app.pool, series, &json!({"Tvdb": "1"}))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, is_default) \
         VALUES ($1, 0, 'Video', 'h264', 1920, 1080, true)",
    )
    .bind(episode)
    .execute(&app.pool)
    .await
    .unwrap();
    (Tree(dir), lib, series, season, episode)
}

fn shows_settings(url: &str) -> LinkSettings {
    let mut s = settings(url, "secret");
    s.libraries = vec![LibraryMap {
        remote: "Shows".into(),
        local: Some("Shows".into()),
    }];
    s
}

/// TV shows merge by series id, then season and episode numbers: a show the
/// server holds gains the friend's episodes and seasons it lacks, and the
/// friend's copy of an episode it holds; a show it lacks is listed whole.
#[tokio::test]
async fn a_friends_shows_merge_into_the_local_ones() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, series_id, season_id, episode_id) = local_shows(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().shows = vec![
        series("s1", "Show", "1"),
        season("s1a", "s1", 1),
        season("s1b", "s1", 2),
        episode("e11", "s1", "s1a", 1, 1, 2160),
        episode("e12", "s1", "s1a", 1, 2, 1080),
        episode("e21", "s1", "s1b", 2, 1, 1080),
        series("s2", "Their Show", "2"),
        season("s2a", "s2", 1),
        episode("e31", "s2", "s2a", 1, 1, 720),
    ];
    let url = fake_jellyfin(remote.clone()).await;
    let link = app
        .links
        .register(&app.pool, shows_settings(&url))
        .await
        .unwrap();
    let report = app.links.sync(&app.pool, link).await.expect("sync");
    assert_eq!(report.added, 9, "{report:?}");

    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Series"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Show", "Their Show"], "{list}");

    let (_, seasons) = app
        .get(&format!("/Shows/{series_id}/Seasons?UserId={uid}"), &token)
        .await;
    assert_eq!(names(&seasons), vec!["Season 1", "Season 2"], "{seasons}");

    let (_, eps) = app
        .get(
            &format!("/Shows/{series_id}/Episodes?UserId={uid}&SeasonId={season_id}"),
            &token,
        )
        .await;
    assert_eq!(names(&eps), vec!["Episode 1x2", "Show S01E01"], "{eps}");
    let (_, all) = app
        .get(&format!("/Shows/{series_id}/Episodes?UserId={uid}"), &token)
        .await;
    assert_eq!(all["Items"].as_array().map(Vec::len), Some(3), "{all}");
    let friends_s2 = all["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["Name"] == "Episode 2x1")
        .unwrap_or_else(|| panic!("{all}"));
    assert!(same(&friends_s2["SeriesId"], series_id), "{friends_s2}");
    assert_eq!(friends_s2["SeriesName"], "Show", "{friends_s2}");

    // The episode both hold: one episode, the friend's 2160p copy first.
    let (_, ep) = app
        .get(&format!("/Users/{uid}/Items/{episode_id}"), &token)
        .await;
    let sources = ep["MediaSources"]
        .as_array()
        .unwrap_or_else(|| panic!("{ep}"));
    assert_eq!(sources.len(), 2, "{ep}");
    assert!(
        sources[0]["Name"]
            .as_str()
            .is_some_and(|n| n.contains("Alex")),
        "{ep}"
    );
    assert!(same(&sources[1]["Id"], episode_id), "{ep}");

    // Their own show is whole: its season, its episode, played through here.
    let theirs = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Their Show")
        .unwrap()["Id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, their_eps) = app
        .get(&format!("/Shows/{theirs}/Episodes?UserId={uid}"), &token)
        .await;
    assert_eq!(names(&their_eps), vec!["Episode 1x1"], "{their_eps}");
    assert_eq!(
        their_eps["Items"][0]["SeriesName"], "Their Show",
        "{their_eps}"
    );
    let their_ep = their_eps["Items"][0]["Id"].as_str().unwrap().to_string();
    let (status, _, bytes) = app
        .get_bytes(&format!("/Videos/{their_ep}/stream?static=true"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, FILM_BYTES);

    // The friend drops S01E02: the season keeps the local episode only.
    remote.lock().unwrap().shows.retain(|i| i["Id"] != "e12");
    let report = app.links.sync(&app.pool, link).await.expect("sync again");
    assert_eq!(report.removed, 1, "{report:?}");
    let (_, eps) = app
        .get(
            &format!("/Shows/{series_id}/Episodes?UserId={uid}&SeasonId={season_id}"),
            &token,
        )
        .await;
    assert_eq!(names(&eps), vec!["Show S01E01"], "{eps}");
    app.cleanup().await;
}

/// A client that cannot play a friend's copy as is gets the friend's
/// transcode, relayed through this server (roadmap 16b.6): the friend's
/// server converts, the client only ever sees this server's URLs and its own
/// token, never the link's.
#[tokio::test]
async fn a_friends_copy_is_transcoded_by_the_friend_through_this_server() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, _lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    remote.lock().unwrap().films = vec![film("r200", "Only Theirs", "200", "e1", 3840, 2160)];
    let url = fake_jellyfin(remote.clone()).await;
    let link = app
        .links
        .register(&app.pool, settings(&url, "secret"))
        .await
        .unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");
    let theirs: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE remote_id = 'r200'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // An H.264-only client: the friend's HEVC copy needs converting.
    let profile = json!({
        "DeviceProfile": {
            "Name": "h264 only",
            "DirectPlayProfiles": [{"Container": "mp4", "VideoCodec": "h264", "AudioCodec": "aac", "Type": "Video"}],
            "TranscodingProfiles": [{"Container": "ts", "Type": "Video", "Protocol": "hls",
                                     "VideoCodec": "h264", "AudioCodec": "aac", "Context": "Streaming"}]
        },
        "StartTimeTicks": 600_000_000_i64,
        "UserId": uid
    });
    let (status, info) = app
        .post(
            &format!("/Items/{theirs}/PlaybackInfo?UserId={uid}"),
            &profile,
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    let source = &info["MediaSources"][0];
    assert_eq!(source["SupportsDirectPlay"], false, "{info}");
    assert_eq!(source["SupportsTranscoding"], true, "{info}");
    let tc = source["TranscodingUrl"]
        .as_str()
        .unwrap_or_else(|| panic!("{info}"));
    assert!(
        tc.starts_with(&format!("/LinkRelay/{link}/videos/r200/master.m3u8?")),
        "{tc}"
    );
    assert!(
        !tc.contains("tok"),
        "the link's token never reaches a client: {tc}"
    );
    assert_eq!(source["TranscodingSubProtocol"], "hls", "{info}");

    // The friend was asked for the copy with the client's profile and start,
    // to convert (not to play as is), and not for this server's user.
    {
        let r = remote.lock().unwrap();
        let body = r.playback_bodies.last().expect("asked the friend");
        assert_eq!(body["MediaSourceId"], "r200", "{body}");
        assert_eq!(body["EnableDirectPlay"], false, "{body}");
        assert_eq!(body["StartTimeTicks"], 600_000_000_i64, "{body}");
        assert_eq!(body["DeviceProfile"]["Name"], "h264 only", "{body}");
        assert!(body.get("UserId").is_none_or(Value::is_null), "{body}");
    }

    // The playlists come through here, with the client's token, never the link's.
    let (status, _, master) = app.get_raw(&format!("{tc}&api_key={token}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{master}");
    assert!(!master.contains("tok"), "{master}");
    let variant_line = master
        .lines()
        .find(|l| l.starts_with("main.m3u8"))
        .unwrap_or_else(|| panic!("{master}"));
    assert!(
        variant_line.contains(&format!("api_key={token}")),
        "{variant_line}"
    );
    let variant_url = format!("/LinkRelay/{link}/videos/r200/{variant_line}");
    let (status, _, variant) = app.get_raw(&variant_url, &token).await;
    assert_eq!(status, StatusCode::OK, "{variant}");
    assert!(!variant.contains("tok"), "{variant}");
    assert!(
        variant.contains(&format!(
            "URI=\"hls1/main/-1.mp4?runtimeTicks=0&api_key={token}\""
        )),
        "{variant}"
    );
    let segment_line = variant
        .lines()
        .find(|l| l.starts_with("hls1/main/0.mp4"))
        .unwrap_or_else(|| panic!("{variant}"));
    let (status, _, bytes) = app
        .get_bytes(
            &format!("/LinkRelay/{link}/videos/r200/{segment_line}"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"SEGMENT 0.mp4");

    // The relay reaches only the link's copies' media, nothing else there.
    let (status, _, _) = app
        .get_raw(&format!("/LinkRelay/{link}/Users"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .get_raw(
            &format!("/LinkRelay/{link}/videos/unknown/master.m3u8"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .get_raw(&format!("/LinkRelay/{link}/videos/r200/master.m3u8"), "")
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}
