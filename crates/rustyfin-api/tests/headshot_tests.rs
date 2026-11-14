//! Actor headshots come from one shared folder, one file per person.
//!
//! Owner (2026-11-14): "we should be pulling actor headshots too and using those
//! in the cast and crew lists. Those should be cached collectively, not with each
//! piece of media and shared across media." The `rustyfin` metadata CLI downloads
//! `<people_cache_dir>/<name>.jpg` once per person; the server reads that folder
//! when it answers, so a headshot shows in every film and show the person is in
//! without a rescan.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use rustyfin_common::images::headshot_file_name;
use rustyfin_db::repo::people;
use std::path::PathBuf;
use uuid::Uuid;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let d = std::env::temp_dir().join(format!("rustyfin-people-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Any real image will do: the poster placeholder.
fn png(dir: &Dir) -> Vec<u8> {
    let made = rustyfin_media::image_processing::Placeholder::Poster
        .ensure_file(&dir.0.join(".made"))
        .unwrap();
    std::fs::read(made).unwrap()
}

async fn cast(app: &common::TestApp, item: Uuid, name: &str) -> Uuid {
    let person = people::find_or_create(&app.pool, name, "Actor", None, None)
        .await
        .unwrap();
    people::link_to_item(&app.pool, item, person.id, Some("Someone"), Some(0))
        .await
        .unwrap();
    person.id
}

#[test]
fn a_headshot_file_is_named_after_the_person() {
    assert_eq!(
        headshot_file_name("Bryan Cranston").as_deref(),
        Some("bryan-cranston.jpg")
    );
    assert_eq!(
        headshot_file_name("  Penélope  Cruz ").as_deref(),
        Some("penélope-cruz.jpg")
    );
    assert_eq!(
        headshot_file_name("Robert Downey Jr.").as_deref(),
        Some("robert-downey-jr.jpg")
    );
    assert_eq!(
        headshot_file_name("../../etc/passwd").as_deref(),
        Some("etc-passwd.jpg")
    );
    assert_eq!(headshot_file_name(" ./ "), None);
}

#[tokio::test]
async fn one_headshot_shows_for_the_person_in_every_item() {
    let people_dir = Dir::new();
    let app = common::TestApp::with_people_cache_dir(people_dir.0.clone()).await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let first = app.create_media_item("First", "Movie", lib, None).await;
    let second = app.create_media_item("Second", "Movie", lib, None).await;
    let actor = cast(&app, first, "Bryan Cranston").await;
    assert_eq!(
        cast(&app, second, "Bryan Cranston").await,
        actor,
        "one person row"
    );
    cast(&app, first, "Nobody Pictured").await;
    let bytes = png(&people_dir);
    std::fs::write(people_dir.0.join("bryan-cranston.jpg"), &bytes).unwrap();

    for item in [first, second] {
        let (status, body) = app.get(&format!("/Items/{item}"), &token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let people = body["People"].as_array().expect("people");
        let bryan = people
            .iter()
            .find(|p| p["Name"] == "Bryan Cranston")
            .unwrap();
        let tag = bryan["PrimaryImageTag"].as_str().expect("a headshot tag");
        let nobody = people
            .iter()
            .find(|p| p["Name"] == "Nobody Pictured")
            .unwrap();
        assert!(nobody["PrimaryImageTag"].is_null(), "{nobody}");

        let (status, _, got) = app
            .get_bytes(&format!("/Items/{actor}/Images/Primary?tag={tag}"), &token)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, bytes, "the shared file, served as is");
    }

    let (status, _, got) = app
        .get_bytes("/Persons/Bryan%20Cranston/Images/Primary", &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, bytes);
    app.cleanup().await;
}
