//! `.rustyfin.toml`: optional per-title settings in a film's or show's folder
//! (owner, 2026-11-15). Hand-written keys override the NFO and folder name;
//! `rustyfin artwork` records when TMDB had no art, keeping the owner's
//! comments and other keys.
use chrono::{TimeZone, Utc};
use rustyfin_common::title_settings::{self, TitleSettings};
use std::path::PathBuf;
use std::time::Duration;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let d = std::env::temp_dir().join(format!(
            "rf-ts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn write(&self, body: &str) {
        std::fs::write(self.0.join(".rustyfin.toml"), body).unwrap();
    }

    fn read(&self) -> String {
        std::fs::read_to_string(self.0.join(".rustyfin.toml")).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn no_file_means_no_settings() {
    let d = Dir::new();
    assert_eq!(title_settings::load(&d.0), None);
}

#[test]
fn ids_title_and_artwork_settings_are_read() {
    let d = Dir::new();
    d.write(
        r#"# my notes
[ids]
tmdb = 802217
tvdb = 393189
imdb = "tt11540284"

[metadata]
title = "8-Bit Christmas"

[artwork]
skip = true
missing_checked = 2026-11-15T08:00:00Z
"#,
    );
    let s = title_settings::load(&d.0).expect("settings");
    assert_eq!(s.ids.tmdb, Some(802217));
    assert_eq!(s.ids.tvdb, Some(393189));
    assert_eq!(s.ids.imdb.as_deref(), Some("tt11540284"));
    assert_eq!(s.metadata.title.as_deref(), Some("8-Bit Christmas"));
    assert!(s.artwork.skip);
    assert_eq!(
        s.missing_checked_at(),
        Some(Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap())
    );
}

#[test]
fn a_broken_or_misspelt_file_is_ignored() {
    let d = Dir::new();
    d.write("[ids\ntmdb = 1\n");
    assert_eq!(title_settings::load(&d.0), None, "not TOML");
    d.write("[ids]\ntmbd = 1\n");
    assert_eq!(title_settings::load(&d.0), None, "unknown key");
}

#[test]
fn missing_art_is_remembered_for_thirty_days() {
    let checked = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let s = TitleSettings {
        artwork: title_settings::Artwork {
            missing_checked: Some(checked),
            ..Default::default()
        },
        ..Default::default()
    };
    let month = Duration::from_secs(30 * 86_400);
    let day = chrono::Duration::days(1);
    assert!(s.missing_checked_within(checked + day * 29, month));
    assert!(!s.missing_checked_within(checked + day * 31, month));
    assert!(!TitleSettings::default().missing_checked_within(checked, month));
}

#[test]
fn recording_missing_art_creates_the_file() {
    let d = Dir::new();
    let when = Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap();
    title_settings::record_missing_checked(&d.0, when).unwrap();
    let body = d.read();
    assert!(
        body.contains("[artwork]\nmissing_checked = 2026-11-15T08:00:00Z\n"),
        "{body}"
    );
    assert!(body.starts_with('#'), "explains itself: {body}");
    assert_eq!(
        title_settings::load(&d.0).unwrap().missing_checked_at(),
        Some(when)
    );
}

#[test]
fn recording_missing_art_keeps_the_owners_comments_and_keys() {
    let d = Dir::new();
    let mine = "# wrong match in the NFO\n[ids]\ntmdb = 5 # the right one\n";
    d.write(mine);
    let when = Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap();
    title_settings::record_missing_checked(&d.0, when).unwrap();
    let body = d.read();
    assert!(body.starts_with(mine), "{body}");
    assert_eq!(title_settings::load(&d.0).unwrap().ids.tmdb, Some(5));

    // An [artwork] table already there gains the key; a second run replaces it.
    d.write("[artwork]\nskip = false # keep\n\n[ids]\ntmdb = 5\n");
    title_settings::record_missing_checked(&d.0, when).unwrap();
    let later = Utc.with_ymd_and_hms(2026, 12, 20, 9, 30, 0).unwrap();
    title_settings::record_missing_checked(&d.0, later).unwrap();
    let body = d.read();
    assert_eq!(body.matches("missing_checked").count(), 1, "{body}");
    assert!(body.contains("skip = false # keep"), "{body}");
    let s = title_settings::load(&d.0).unwrap();
    assert_eq!(s.missing_checked_at(), Some(later));
    assert_eq!(s.ids.tmdb, Some(5));
}

#[test]
fn a_file_that_does_not_parse_is_never_overwritten() {
    let d = Dir::new();
    d.write("[ids\nbroken");
    let when = Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap();
    assert!(title_settings::record_missing_checked(&d.0, when).is_err());
    assert_eq!(d.read(), "[ids\nbroken");
}

#[test]
fn the_lyrics_check_is_recorded_beside_other_settings() {
    let dir = std::env::temp_dir().join(format!("rustyfin-ts-lyrics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".rustyfin.toml"),
        "# mine\n[artwork]\nskip = true\n",
    )
    .unwrap();
    rustyfin_common::title_settings::record_lyrics_checked_now(&dir).unwrap();
    rustyfin_common::title_settings::record_lyrics_checked_now(&dir).unwrap();
    let text = std::fs::read_to_string(dir.join(".rustyfin.toml")).unwrap();
    assert!(
        text.starts_with("# mine\n[artwork]\nskip = true\n"),
        "{text}"
    );
    assert_eq!(text.matches("checked =").count(), 1, "{text}");
    let s = rustyfin_common::title_settings::load(&dir).unwrap();
    assert!(s.artwork.skip);
    assert!(s.lyrics_checked_recently(std::time::Duration::from_secs(60)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_collection_found_on_tmdb_is_recorded_beside_other_settings() {
    // Owner, 2026-11-16: `rustyfin artwork --check-collections` asks TMDB which
    // collection a film is in when its NFO does not say, and keeps the answer
    // in the title's settings (NFOs belong to Radarr).
    let d = Dir::new();
    d.write("# mine\n[artwork]\nskip = true\n");
    let name = r#"28 Days/Weeks/Years Later "Collection""#;
    title_settings::record_collection_checked(&d.0, Some((1565, name))).unwrap();
    title_settings::record_collection_checked(&d.0, Some((1565, name))).unwrap();
    let text = d.read();
    assert!(
        text.starts_with("# mine\n[artwork]\nskip = true\n"),
        "{text}"
    );
    assert_eq!(text.matches("tmdb =").count(), 1, "{text}");
    let s = title_settings::load(&d.0).unwrap();
    assert!(s.artwork.skip);
    assert_eq!(s.collection.tmdb, Some(1565));
    assert_eq!(s.collection.name.as_deref(), Some(name));
    assert!(s.collection_checked_recently(Duration::from_secs(60)));
}

#[test]
fn a_film_in_no_collection_is_remembered_as_checked() {
    let d = Dir::new();
    title_settings::record_collection_checked(&d.0, None).unwrap();
    let s = title_settings::load(&d.0).unwrap();
    assert_eq!(s.collection.tmdb, None);
    assert!(s.collection_checked_recently(Duration::from_secs(60)));
    assert!(!TitleSettings::default().collection_checked_recently(Duration::from_secs(60)));
}
