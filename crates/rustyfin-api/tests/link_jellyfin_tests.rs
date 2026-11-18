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
    films: Vec<Value>,
    /// Requests seen, as "METHOD path?query".
    seen: Vec<String>,
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
        {"Id": "lib2", "Name": "Home Videos", "CollectionType": "homevideos"}
    ], "TotalRecordCount": 2}))
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
    let films: Vec<Value> = if let Some(ids) = q.get("Ids") {
        let ids: Vec<&str> = ids.split(',').collect();
        r.films
            .iter()
            .filter(|f| ids.contains(&f["Id"].as_str().unwrap()))
            .cloned()
            .collect()
    } else {
        assert_eq!(q.get("ParentId").map(String::as_str), Some("lib1"), "{q:?}");
        assert_eq!(q.get("IncludeItemTypes").map(String::as_str), Some("Movie"));
        let start: usize = q
            .get("StartIndex")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let limit: usize = q
            .get("Limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        r.films
            .iter()
            .skip(start)
            .take(limit)
            .map(|f| json!({"Id": f["Id"], "Etag": f["Etag"], "Type": "Movie"}))
            .collect()
    };
    let total = r.films.len();
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

/// A fake Jellyfin 10.10 on a free port; returns its base URL.
async fn fake_jellyfin(remote: Shared) -> String {
    let app = Router::new()
        .route("/Users/AuthenticateByName", post(authenticate))
        .route("/UserViews", get(views))
        .route("/Items", get(items))
        .route("/Videos/{id}/stream", get(stream))
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
            local: "Movies".into(),
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

#[tokio::test]
async fn a_library_the_link_cannot_see_is_an_error_not_an_empty_sync() {
    let app = common::TestApp::new().await;
    let (_tree, _lib, _local) = local_library(&app).await;
    let remote: Shared = Default::default();
    let url = fake_jellyfin(remote).await;
    let mut s = settings(&url, "secret");
    s.libraries[0].remote = "Missing".into();
    let link = app.links.register(&app.pool, s).await.unwrap();
    let err = app.links.sync(&app.pool, link).await.unwrap_err();
    assert!(err.to_string().contains("Missing"), "{err}");
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
