//! Cover art embedded in a track, for albums with no cover file (owner,
//! 2026-11-16: new Lidarr albums showed blank in Plezy; Jellyfin uses the
//! first track's embedded picture when the folder has none).
use std::path::{Path, PathBuf};

use rustyfin_media::image_processing::embedded_cover;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio")
        .join(name)
}

fn cache() -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rustyfin-cover-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn a_tracks_embedded_cover_is_written_once_into_the_cache() {
    let dir = cache();
    let path = embedded_cover(&fixture("sine.mp3"), &dir, "cover-abc")
        .await
        .expect("the mp3 fixture carries a 64x64 jpeg");
    assert!(path.starts_with(&dir));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "a JPEG");
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert_eq!((w, h), (64, 64));
    // Asked again: the same file, not rewritten.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let again = embedded_cover(&fixture("sine.mp3"), &dir, "cover-abc")
        .await
        .unwrap();
    assert_eq!(again, path);
    assert_eq!(
        std::fs::metadata(&again).unwrap().modified().unwrap(),
        modified
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_track_without_a_cover_or_a_missing_file_gives_none() {
    let dir = cache();
    assert!(
        embedded_cover(&fixture("sine.flac"), &dir, "cover-x")
            .await
            .is_none()
    );
    assert!(
        embedded_cover(&dir.join("missing.mp3"), &dir, "cover-y")
            .await
            .is_none()
    );
    std::fs::remove_dir_all(&dir).ok();
}
