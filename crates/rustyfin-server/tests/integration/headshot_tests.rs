//! Actor headshots come from one shared folder, one file per person.
//!
//! Owner (2026-11-14): "we should be pulling actor headshots too and using those
//! in the cast and crew lists. Those should be cached collectively, not with each
//! piece of media and shared across media", and "we can't use people's names,
//! we must use either the tmdb id ... or content addressed storage" ("tmdb id and
//! hash"). A headshot is `<people_cache_dir>/tmdb-<id>.jpg` when the person's
//! TMDB id is known, else `img-<sha256 of the photo's address>.jpg`. The
//! `rustyfin` metadata CLI downloads it once; the server reads the folder when
//! it answers, so a headshot shows in every film the person is in without a
//! rescan.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::people;
use rustyfin_services::headshot_file_name;
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

async fn cast(
    app: &common::TestApp,
    item: Uuid,
    name: &str,
    url: Option<&str>,
    tmdb: Option<&str>,
) -> Uuid {
    let person = people::find_or_create(&app.pool, name, "Actor", url, tmdb)
        .await
        .unwrap();
    people::link_to_item(&app.pool, item, person.id, Some("Someone"), Some(0))
        .await
        .unwrap();
    person.id
}

const BELL: &str = "https://image.tmdb.org/t/p/original/kSjVXfF1ODB6RmFMVJ8XOxWyYCj.jpg";

#[test]
fn a_headshot_is_named_by_tmdb_id_else_by_a_hash_of_the_photo() {
    // The TMDB id wins.
    assert_eq!(
        headshot_file_name(Some("tmdb-17052"), Some(BELL)).as_deref(),
        Some("tmdb-17052.jpg")
    );
    // Without one: a hash of the photo's address, the same at every TMDB size
    // and scheme, never the person's name.
    let hashed = headshot_file_name(None, Some(BELL)).unwrap();
    assert!(
        hashed.starts_with("img-") && hashed.ends_with(".jpg"),
        "{hashed}"
    );
    assert_eq!(hashed.len(), "img-".len() + 64 + ".jpg".len(), "{hashed}");
    assert!(hashed[4..68].chars().all(|c| c.is_ascii_hexdigit()));
    for same in [
        "http://image.tmdb.org/t/p/h632/kSjVXfF1ODB6RmFMVJ8XOxWyYCj.jpg",
        "https://image.tmdb.org/t/p/w185/kSjVXfF1ODB6RmFMVJ8XOxWyYCj.jpg",
    ] {
        assert_eq!(
            headshot_file_name(None, Some(same)).as_deref(),
            Some(hashed.as_str())
        );
    }
    let other = headshot_file_name(None, Some("https://artworks.thetvdb.com/a/b.jpg")).unwrap();
    assert_ne!(other, hashed);
    assert!(other.starts_with("img-"));
    // An id that is not a TMDB number is ignored, never put in a path.
    assert_eq!(
        headshot_file_name(Some("tmdb-../../x"), Some(BELL)).as_deref(),
        Some(hashed.as_str())
    );
    assert_eq!(headshot_file_name(Some("imdb-nm1"), None), None);
    assert_eq!(headshot_file_name(None, Some("  ")), None);
    assert_eq!(headshot_file_name(None, None), None);
}

#[tokio::test]
async fn one_headshot_shows_for_the_person_in_every_item() {
    let people_dir = Dir::new();
    let app = common::TestApp::with_people_cache_dir(people_dir.0.clone()).await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let first = app.create_media_item("First", "Movie", lib, None).await;
    let second = app.create_media_item("Second", "Movie", lib, None).await;
    let bell = cast(&app, first, "Kristen Bell", Some(BELL), None).await;
    assert_eq!(
        cast(&app, second, "Kristen Bell", Some(BELL), None).await,
        bell,
        "one person row"
    );
    let menzel = cast(&app, first, "Idina Menzel", None, Some("tmdb-19394")).await;
    cast(&app, first, "Nobody Pictured", None, None).await;
    let bytes = png(&people_dir);
    let bell_file = headshot_file_name(None, Some(BELL)).unwrap();
    std::fs::write(people_dir.0.join(&bell_file), &bytes).unwrap();
    std::fs::write(people_dir.0.join("tmdb-19394.jpg"), &bytes).unwrap();
    // A file named after a person is not a headshot any more.
    std::fs::write(people_dir.0.join("nobody-pictured.jpg"), &bytes).unwrap();

    for item in [first, second] {
        let (status, body) = app.get(&format!("/Items/{item}"), &token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let people = body["People"].as_array().expect("people");
        let tag_of = |name: &str| {
            people
                .iter()
                .find(|p| p["Name"] == name)
                .map(|p| p["PrimaryImageTag"].clone())
        };
        let tag = tag_of("Kristen Bell").unwrap();
        let tag = tag.as_str().expect("a headshot tag");
        if item == first {
            assert!(tag_of("Idina Menzel").unwrap().is_string(), "{body}");
            assert!(tag_of("Nobody Pictured").unwrap().is_null(), "{body}");
        }

        let (status, _, got) = app
            .get_bytes(&format!("/Items/{bell}/Images/Primary?tag={tag}"), &token)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(got, bytes, "the shared file, served as is");
    }

    let (status, _, got) = app
        .get_bytes(&format!("/Items/{menzel}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, bytes);
    let (status, _, got) = app
        .get_bytes("/Persons/Kristen%20Bell/Images/Primary", &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, bytes);
    let (status, _, _) = app
        .get_bytes("/Persons/Nobody%20Pictured/Images/Primary", &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

const LEDGER: &str = "https://image.tmdb.org/t/p/original/5Y9HnYYa9jF4NunY9lSgJGjSe8E.jpg";
const LEDGER_LOCAL: &str = "/config/metadata/People/H/Heath Ledger/folder.jpg";

#[tokio::test]
async fn a_web_photo_replaces_a_local_path_and_stays() {
    // Owner (2026-11-21): 8,174 people kept a photo path from Jellyfin-era NFOs
    // (a file in Jellyfin's own volume) after their NFOs gave them TMDB
    // addresses, so nothing could ever download their headshot.
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let old = app.create_media_item("Old NFO", "Movie", lib, None).await;
    let new = app.create_media_item("New NFO", "Movie", lib, None).await;
    let later = app.create_media_item("Later NFO", "Movie", lib, None).await;

    let id = cast(&app, old, "Heath Ledger", Some(LEDGER_LOCAL), None).await;
    cast(&app, new, "Heath Ledger", Some(LEDGER), None).await;
    let row = people::find_by_id(&app.pool, id).await.unwrap().unwrap();
    assert_eq!(
        row.image_url.as_deref(),
        Some(LEDGER),
        "the web address wins"
    );

    // A web address is kept once stored: its file name must not move under a
    // headshot already downloaded for it, and a local path never displaces it.
    cast(&app, later, "Heath Ledger", Some(BELL), None).await;
    cast(&app, later, "Heath Ledger", Some(LEDGER_LOCAL), None).await;
    let row = people::find_by_id(&app.pool, id).await.unwrap().unwrap();
    assert_eq!(row.image_url.as_deref(), Some(LEDGER));
    app.cleanup().await;
}
