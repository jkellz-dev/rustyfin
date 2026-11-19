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
    /// The server is down: everything answers 503.
    down: bool,
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
    token("authorization").contains("Token=\"linksecret\"") || token("x-emby-token") == "linksecret"
}

async fn authenticate(State(remote): State<Shared>, body: axum::Json<Value>) -> Response {
    remote
        .lock()
        .unwrap()
        .seen
        .push("POST /Users/AuthenticateByName".into());
    if body["Username"] == "link" && body["Pw"] == "secret" {
        axum::Json(json!({"AccessToken": "linksecret", "User": {"Id": "u1", "Name": "link"}}))
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
                "/videos/{id}/master.m3u8?MediaSourceId={source}&PlaySessionId=p1&VideoCodec=h264&api_key=linksecret"
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
            .all(|(k, v)| !k.eq_ignore_ascii_case("api_key") || v == "linksecret")
}

async fn master(headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !hls_allowed(&headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [("content-type", "application/vnd.apple.mpegurl")],
        "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000000\nmain.m3u8?MediaSourceId=r200&PlaySessionId=p1&api_key=linksecret\n",
    )
        .into_response()
}

async fn variant(headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !hls_allowed(&headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [("content-type", "application/vnd.apple.mpegurl")],
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MAP:URI=\"hls1/main/-1.mp4?runtimeTicks=0&api_key=linksecret\"\n#EXTINF:6.0,\nhls1/main/0.mp4?runtimeTicks=0\n#EXT-X-ENDLIST\n",
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
        .with_state(remote.clone())
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let down = remote.lock().unwrap().down;
                async move {
                    if down {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        next.run(req).await
                    }
                }
            },
        ));
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
        oidc: None,
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
        tc.starts_with(&format!(
            "/api/v1/links/{link}/relay/videos/r200/master.m3u8?"
        )),
        "{tc}"
    );
    assert!(
        !tc.contains("linksecret"),
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
    assert!(!master.contains("linksecret"), "{master}");
    let variant_line = master
        .lines()
        .find(|l| l.starts_with("main.m3u8"))
        .unwrap_or_else(|| panic!("{master}"));
    assert!(
        variant_line.contains(&format!("api_key={token}")),
        "{variant_line}"
    );
    let variant_url = format!("/api/v1/links/{link}/relay/videos/r200/{variant_line}");
    let (status, _, variant) = app.get_raw(&variant_url, &token).await;
    assert_eq!(status, StatusCode::OK, "{variant}");
    assert!(!variant.contains("linksecret"), "{variant}");
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
            &format!("/api/v1/links/{link}/relay/videos/r200/{segment_line}"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"SEGMENT 0.mp4");

    // The relay reaches only the link's copies' media, nothing else there.
    let (status, _, _) = app
        .get_raw(&format!("/api/v1/links/{link}/relay/Users"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .get_raw(
            &format!("/api/v1/links/{link}/relay/videos/unknown/master.m3u8"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .get_raw(
            &format!("/api/v1/links/{link}/relay/videos/r200/master.m3u8"),
            "",
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

/// The admin's Links page (roadmap 16b): each link's last sync, its error,
/// how many titles and copies it brought, and a sync on demand.
#[tokio::test]
async fn the_admin_sees_each_link_and_can_sync_it_now() {
    let app = common::TestApp::new().await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (_viewer, viewer_token) = app.create_user("viewer", false).await;
    let (_tree, _lib, _local) = local_library(&app).await;
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

    let (status, _) = app.get("/api/v1/links", &viewer_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, list) = app.get("/api/v1/links", &admin_token).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let entry = &list[0];
    assert_eq!(entry["name"], "Alex", "{list}");
    assert_eq!(entry["kind"], "jellyfin", "{list}");
    assert!(entry["last_sync_at"].is_null(), "never synced yet: {list}");
    assert_eq!(entry["configured"], true, "{list}");

    // Sync now: answered at once, done in the background.
    let (status, _) = app
        .post(
            &format!("/api/v1/links/{link}/sync"),
            &json!({}),
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut synced = Value::Null;
    for _ in 0..50 {
        let (_, list) = app.get("/api/v1/links", &admin_token).await;
        if list[0]["last_sync_at"].is_string() && list[0]["syncing"] == false {
            synced = list[0].clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(synced.is_object(), "the sync finished");
    // "Shared Film" merged into the local film: one title of its own, two copies.
    assert_eq!(synced["titles"], 1, "{synced}");
    assert_eq!(synced["copies"], 2, "{synced}");
    assert!(synced["last_error"].is_null(), "{synced}");

    let (status, _) = app
        .post(
            &format!("/api/v1/links/{}/sync", Uuid::new_v4()),
            &json!({}),
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .post(
            &format!("/api/v1/links/{link}/sync"),
            &json!({}),
            &viewer_token,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

/// A link taken out of the config file is removed when the server starts
/// (roadmap 16b): its copies, the library of its own, and the artwork local
/// films borrowed from it. A link still named in the config stays, even one
/// whose settings are broken for now. Local films stay.
#[tokio::test]
async fn a_link_taken_out_of_the_config_is_removed_at_start() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, local) = local_library(&app).await;
    let remote: Shared = Default::default();
    {
        let mut r = remote.lock().unwrap();
        r.films = vec![
            film("r100", "Shared Film", "100", "e1", 3840, 2160),
            film("r200", "Only Theirs", "200", "e1", 1920, 1080),
        ];
        r.videos = vec![video("v1", "Beach")];
    }
    let url = fake_jellyfin(remote).await;
    let mut s = settings(&url, "secret");
    s.libraries.push(LibraryMap {
        remote: "Home Videos".into(),
        local: None,
    });
    let link = app.links.register(&app.pool, s).await.unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");
    let (_, mine) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    assert!(mine["ImageTags"]["Primary"].is_string(), "borrowed: {mine}");

    // Still named in the config: nothing goes.
    let removed = rustyfin_db::repo::links::remove_unconfigured(&app.pool, &["Alex".to_string()])
        .await
        .unwrap();
    assert!(removed.is_empty(), "{removed:?}");

    let removed = rustyfin_db::repo::links::remove_unconfigured(&app.pool, &[])
        .await
        .unwrap();
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert_eq!(removed[0].0, "Alex");
    assert_eq!(removed[0].1, 3, "two films and a video: {removed:?}");

    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM links) + (SELECT COUNT(*) FROM media_items WHERE link_id IS NOT NULL) \
         + (SELECT COUNT(*) FROM libraries WHERE link_id IS NOT NULL) \
         + (SELECT COUNT(*) FROM images WHERE path LIKE 'link://%')",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
    let (_, list) = app
        .get(
            &format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Shared Film"], "{list}");
    let (_, mine) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    assert_eq!(
        mine["MediaSources"].as_array().map(Vec::len),
        Some(1),
        "{mine}"
    );
    app.cleanup().await;
}

/// The next "DisplayMessage" a session gets, as (header, text).
async fn notice(
    rx: &mut tokio::sync::broadcast::Receiver<rustyfin_models::dto::SessionCommand>,
) -> (String, String) {
    let wait = std::time::Duration::from_secs(5);
    loop {
        match tokio::time::timeout(wait, rx.recv()).await {
            Ok(Ok(rustyfin_models::dto::SessionCommand::GeneralCommand(c)))
                if c.name == "DisplayMessage" =>
            {
                return (
                    c.arguments.get("Header").cloned().unwrap_or_default(),
                    c.arguments.get("Text").cloned().unwrap_or_default(),
                );
            }
            Ok(Ok(_)) => continue,
            other => panic!("no notice: {other:?}"),
        }
    }
}

/// Owner, 2026-11-19: "When a friends server is down, the user should be
/// notified. Their titles should be omitted from the views." Every view
/// leaves out what only that friend has (lists, search, Latest, Resume, the
/// item itself, a library of its own) and drops the friend's copy from a title
/// the server holds; users who can see those titles get a message. All of it
/// comes back, with another message, when the friend's server does.
#[tokio::test]
async fn a_friends_titles_leave_the_views_while_their_server_is_down() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", false).await;
    let (_admin, admin_token) = app.create_user("admin", true).await;
    let (_tree, lib, local) = local_library(&app).await;
    let remote: Shared = Default::default();
    {
        let mut r = remote.lock().unwrap();
        r.films = vec![
            film("r100", "Shared Film", "100", "e1", 3840, 2160),
            film("r200", "Only Theirs", "200", "e1", 1920, 1080),
        ];
        r.videos = vec![video("v1", "Beach")];
    }
    let url = fake_jellyfin(remote.clone()).await;
    let mut s = settings(&url, "secret");
    s.libraries.push(LibraryMap {
        remote: "Home Videos".into(),
        local: None,
    });
    let link = app.links.register(&app.pool, s).await.unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");
    let theirs: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE remote_id = 'r200'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_data (user_id, item_id, playback_position_ticks, last_played_date) \
         VALUES ($1, $2, 6000000000, now())",
    )
    .bind(uid)
    .bind(theirs)
    .execute(&app.pool)
    .await
    .unwrap();
    let session: Uuid = sqlx::query_scalar("SELECT id FROM sessions WHERE access_token = $1")
        .bind(&token)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let mut rx = app.session_manager.subscribe(session).await;

    let movies = format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie");
    let resume = format!("/Users/{uid}/Items/Resume");
    let latest = format!("/Users/{uid}/Items/Latest?ParentId={lib}");
    let views = format!("/Users/{uid}/Views");
    let search = "/Search/Hints?SearchTerm=Only";
    let view_names = |v: &Value| -> Vec<String> {
        v["Items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["Name"].as_str().unwrap().to_string())
            .collect()
    };
    let sources = |v: &Value| v["MediaSources"].as_array().map(Vec::len).unwrap_or(0);

    // Up: everything is there.
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    let (_, list) = app.get(&resume, &token).await;
    assert_eq!(names(&list), vec!["Only Theirs"], "{list}");
    let (_, v) = app.get(&views, &token).await;
    assert!(
        view_names(&v).contains(&"Home Videos · Alex".to_string()),
        "{v}"
    );
    let (_, mine) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    assert_eq!(sources(&mine), 2, "{mine}");

    // Down.
    remote.lock().unwrap().down = true;
    assert!(app.links.sync(&app.pool, link).await.is_err());
    let (header, text) = notice(&mut rx).await;
    assert!(!header.is_empty());
    assert!(text.contains("Alex") && text.contains("hidden"), "{text}");
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Shared Film"], "{list}");
    let (status, _) = app
        .get(&format!("/Users/{uid}/Items/{theirs}"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = app.get(&resume, &token).await;
    assert!(names(&list).is_empty(), "{list}");
    let (_, list) = app.get(&latest, &token).await;
    let latest_names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap())
        .collect();
    assert!(!latest_names.contains(&"Only Theirs"), "{list}");
    let (_, hints) = app.get(search, &token).await;
    assert_eq!(hints["TotalRecordCount"], 0, "{hints}");
    let (_, v) = app.get(&views, &token).await;
    assert!(
        !view_names(&v).contains(&"Home Videos · Alex".to_string()),
        "{v}"
    );
    let (_, mine) = app
        .get(&format!("/Users/{uid}/Items/{local}"), &token)
        .await;
    assert_eq!(sources(&mine), 1, "only this server's copy: {mine}");
    let (_, links) = app.get("/api/v1/links", &admin_token).await;
    assert!(links[0]["offline_since"].is_string(), "{links}");
    // Still down: no second message.
    assert!(app.links.recheck_offline(&app.pool).await == 0);
    assert!(rx.try_recv().is_err(), "one message per change");

    // Back.
    remote.lock().unwrap().down = false;
    assert_eq!(app.links.recheck_offline(&app.pool).await, 1);
    let (_, text) = notice(&mut rx).await;
    assert!(text.contains("Alex") && text.contains("back"), "{text}");
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    let (_, list) = app.get(&resume, &token).await;
    assert_eq!(
        names(&list),
        vec!["Only Theirs"],
        "watched state kept: {list}"
    );
    let (_, links) = app.get("/api/v1/links", &admin_token).await;
    assert!(links[0]["offline_since"].is_null(), "{links}");
    app.cleanup().await;
}

/// A title two friends hold stays while one of them is down: the other's
/// copy stands in for it.
#[tokio::test]
async fn a_title_another_friend_holds_stays_while_one_is_down() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, _local) = local_library(&app).await;
    let mut ids = Vec::new();
    let mut remotes = Vec::new();
    for name in ["Alex", "Sam"] {
        let remote: Shared = Default::default();
        remote.lock().unwrap().films = vec![film("r200", "Only Theirs", "200", "e1", 1920, 1080)];
        let url = fake_jellyfin(remote.clone()).await;
        let mut s = settings(&url, "secret");
        s.name = name.into();
        let link = app.links.register(&app.pool, s).await.unwrap();
        app.links.sync(&app.pool, link).await.expect("sync");
        ids.push(link);
        remotes.push(remote);
    }
    // Take down the friend whose copy stands for the title.
    let standing: Uuid = sqlx::query_scalar(
        "SELECT link_id FROM media_items WHERE remote_id = 'r200' AND owner_id IS NULL",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let down = ids.iter().position(|i| *i == standing).unwrap();
    remotes[down].lock().unwrap().down = true;
    assert!(app.links.sync(&app.pool, standing).await.is_err());

    let movies = format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie");
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    let item = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Only Theirs")
        .unwrap()["Id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, film) = app.get(&format!("/Users/{uid}/Items/{item}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{film}");
    assert_eq!(
        film["MediaSources"].as_array().map(Vec::len),
        Some(1),
        "{film}"
    );

    remotes[down].lock().unwrap().down = false;
    app.links.recheck_offline(&app.pool).await;
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Only Theirs", "Shared Film"], "{list}");
    let (_, film) = app.get(&format!("/Users/{uid}/Items/{item}"), &token).await;
    assert_eq!(
        film["MediaSources"].as_array().map(Vec::len),
        Some(2),
        "both copies again: {film}"
    );
    app.cleanup().await;
}

/// The views read `visible_media_items`, a view over `media_items` whose
/// columns are fixed when it is made: a migration adding a column to
/// `media_items` must make the view again.
#[tokio::test]
async fn the_visible_view_has_every_column_of_media_items() {
    let app = common::TestApp::new().await;
    let columns = |table: &'static str| {
        let pool = app.pool.clone();
        async move {
            let names: Vec<String> = sqlx::query_scalar(
                "SELECT column_name::text FROM information_schema.columns \
                 WHERE table_schema = current_schema() AND table_name = $1 ORDER BY ordinal_position",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .unwrap();
            names
        }
    };
    let table = columns("media_items").await;
    assert!(!table.is_empty());
    assert_eq!(columns("visible_media_items").await, table);
    app.cleanup().await;
}

/// Owner, 2026-11-19: "Keep watched history" when a link is removed. What a
/// user had of a friend's title (watched, resume point, favourite) is kept
/// aside when the link goes, and given back when the same title (by provider
/// id) turns up again: on this server, or through another link.
#[tokio::test]
async fn watched_state_outlives_a_removed_link_and_comes_back_with_the_title() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let (_tree, lib, _local) = local_library(&app).await;
    let alex: Shared = Default::default();
    alex.lock().unwrap().films = vec![film("r200", "Only Theirs", "200", "e1", 1920, 1080)];
    let url = fake_jellyfin(alex).await;
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
    sqlx::query(
        "INSERT INTO user_data (user_id, item_id, played, play_count, is_favorite, last_played_date) \
         VALUES ($1, $2, true, 2, true, now())",
    )
    .bind(uid)
    .bind(theirs)
    .execute(&app.pool)
    .await
    .unwrap();

    rustyfin_db::repo::links::remove_unconfigured(&app.pool, &[])
        .await
        .unwrap();
    let movies = format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=Movie");
    let (_, list) = app.get(&movies, &token).await;
    assert_eq!(names(&list), vec!["Shared Film"], "{list}");

    // The same film comes back through another friend.
    let sam: Shared = Default::default();
    sam.lock().unwrap().films = vec![film("s9", "Only Theirs", "200", "e1", 1280, 720)];
    let url = fake_jellyfin(sam).await;
    let mut s = settings(&url, "secret");
    s.name = "Sam".into();
    let link = app.links.register(&app.pool, s).await.unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");
    let (_, list) = app.get(&movies, &token).await;
    let back = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Only Theirs")
        .cloned()
        .unwrap_or_else(|| panic!("{list}"));
    assert_eq!(back["UserData"]["Played"], true, "{back}");
    assert_eq!(back["UserData"]["IsFavorite"], true, "{back}");
    assert_eq!(back["UserData"]["PlayCount"], 2, "{back}");
    let parked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM parked_user_data")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(parked, 0, "given back once");
    app.cleanup().await;
}

/// Owner, 2026-11-19: a setting to prefer a copy that plays as is over a
/// better one that would have to be converted. Off by default: the best copy
/// comes first even when the friend's server has to convert it.
#[tokio::test]
async fn a_copy_that_plays_as_is_can_be_preferred_over_a_better_one() {
    for prefer in [false, true] {
        let app = if prefer {
            common::TestApp::with_prefer_direct_play().await
        } else {
            common::TestApp::new().await
        };
        let (uid, token) = app.create_user("viewer", true).await;
        let (_tree, _lib, local) = local_library(&app).await;
        sqlx::query("UPDATE media_items SET container = 'mkv' WHERE id = $1")
            .bind(local)
            .execute(&app.pool)
            .await
            .unwrap();
        // With an audio track the client plays (one without none never
        // matches a profile's AudioCodec).
        sqlx::query(
            "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, channels, is_default) \
             VALUES ($1, 1, 'Audio', 'aac', 2, true)",
        )
        .bind(local)
        .execute(&app.pool)
        .await
        .unwrap();
        let remote: Shared = Default::default();
        remote.lock().unwrap().films = vec![film("r100", "Shared Film", "100", "e1", 3840, 2160)];
        let url = fake_jellyfin(remote).await;
        let link = app
            .links
            .register(&app.pool, settings(&url, "secret"))
            .await
            .unwrap();
        app.links.sync(&app.pool, link).await.expect("sync");
        // An H.264 client: this server's 1080p H.264 copy plays as is; the
        // friend's 2160p HEVC copy has to be converted (by the friend).
        let profile = json!({
            "DeviceProfile": {
                "Name": "h264 only",
                "DirectPlayProfiles": [{"Container": "mkv,mp4", "VideoCodec": "h264",
                                        "AudioCodec": "aac,eac3", "Type": "Video"}],
                "TranscodingProfiles": [{"Container": "ts", "Type": "Video", "Protocol": "hls",
                                         "VideoCodec": "h264", "AudioCodec": "aac", "Context": "Streaming"}]
            },
            "UserId": uid
        });
        let (status, info) = app
            .post(
                &format!("/Items/{local}/PlaybackInfo?UserId={uid}"),
                &profile,
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{info}");
        let first = &info["MediaSources"][0];
        let mine = first["Id"]
            .as_str()
            .is_some_and(|id| same(&json!(id), local));
        if prefer {
            assert!(
                mine && first["SupportsDirectPlay"] == true,
                "plays as is first: {info}"
            );
        } else {
            assert!(!mine, "the better copy first: {info}");
            assert_eq!(first["SupportsTranscoding"], true, "{info}");
        }
        assert_eq!(
            info["MediaSources"].as_array().map(Vec::len),
            Some(2),
            "{info}"
        );
        app.cleanup().await;
    }
}

/// Owner, 2026-11-19: "Only users who are directly using my instance of
/// rustyfin should see linked libraries." A server linking this one (a user
/// allowed to link, or Rustyfin's link client) sees this server's own media
/// only: never what this server holds from its own links, so two friends who
/// link each other never hand each other's titles back.
#[tokio::test]
async fn a_linking_server_never_sees_what_this_server_links() {
    let app = common::TestApp::new().await;
    let (person, person_token) = app.create_user("viewer", false).await;
    let (friend, friend_token) = app.create_user("friend-server", false).await;
    sqlx::query("UPDATE users SET may_link = true WHERE id = $1")
        .bind(friend)
        .execute(&app.pool)
        .await
        .unwrap();
    let (_, client_token) = app.create_user("other-server", false).await;
    sqlx::query("UPDATE sessions SET device_name = 'Rustyfin link' WHERE access_token = $1")
        .bind(&client_token)
        .execute(&app.pool)
        .await
        .unwrap();
    let (_tree, _lib, local) = local_library(&app).await;
    let remote: Shared = Default::default();
    {
        let mut r = remote.lock().unwrap();
        r.films = vec![
            film("r100", "Shared Film", "100", "e1", 3840, 2160),
            film("r200", "Only Theirs", "200", "e1", 1920, 1080),
        ];
        r.videos = vec![video("v1", "Beach")];
    }
    let url = fake_jellyfin(remote).await;
    let mut s = settings(&url, "secret");
    s.libraries.push(LibraryMap {
        remote: "Home Videos".into(),
        local: None,
    });
    let link = app.links.register(&app.pool, s).await.unwrap();
    app.links.sync(&app.pool, link).await.expect("sync");
    let theirs: Uuid = sqlx::query_scalar("SELECT id FROM media_items WHERE remote_id = 'r200'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Someone using this server sees it all.
    let movies = |uid: Uuid| format!("/Users/{uid}/Items?Recursive=true&IncludeItemTypes=Movie");
    let (_, list) = app.get(&movies(person), &person_token).await;
    // (The friend's home video is a Movie too, in a library of its own.)
    assert_eq!(
        names(&list),
        vec!["Beach", "Only Theirs", "Shared Film"],
        "{list}"
    );
    let (_, info) = app
        .get(&format!("/Items/{local}/PlaybackInfo"), &person_token)
        .await;
    assert_eq!(
        info["MediaSources"].as_array().map(Vec::len),
        Some(2),
        "{info}"
    );

    let other: Uuid = sqlx::query_scalar("SELECT user_id FROM sessions WHERE access_token = $1")
        .bind(&client_token)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    for (uid, token) in [(friend, &friend_token), (other, &client_token)] {
        let (status, list) = app.get(&movies(uid), token).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(
            names(&list),
            vec!["Shared Film"],
            "only this server's own: {list}"
        );
        let (_, views) = app.get("/UserViews", token).await;
        let views: Vec<&str> = views["Items"]
            .as_array()
            .unwrap_or_else(|| panic!("{views}"))
            .iter()
            .filter_map(|v| v["Name"].as_str())
            .collect();
        assert!(!views.contains(&"Home Videos · Alex"), "{views:?}");
        assert!(views.contains(&"Movies"), "{views:?}");
        let (_, info) = app
            .get(&format!("/Items/{local}/PlaybackInfo"), token)
            .await;
        assert_eq!(
            info["MediaSources"].as_array().map(Vec::len),
            Some(1),
            "not the friend's copy: {info}"
        );
        let (status, _) = app.get(&format!("/Items/{theirs}"), token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, hints) = app.get("/Search/Hints?SearchTerm=Only", token).await;
        assert_eq!(
            hints["SearchHints"].as_array().map(Vec::len),
            Some(0),
            "{hints}"
        );
        let (status, _) = app
            .get(
                &format!("/api/v1/links/{link}/relay/videos/r200/stream"),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    // Still all there for the person.
    let (_, list) = app.get(&movies(person), &person_token).await;
    assert_eq!(
        names(&list),
        vec!["Beach", "Only Theirs", "Shared Film"],
        "{list}"
    );
    app.cleanup().await;
}
