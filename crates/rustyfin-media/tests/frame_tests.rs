//! Frames grabbed from a video for an episode's thumbnail: one ffmpeg run per
//! frame however many ask at once, kept in the cache, and nothing left behind
//! when ffmpeg fails. ffmpeg is replaced by a script (the build machine has none).

use rustyfin_media::image_processing::{self, Placeholder};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rustyfin-frame-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A fake ffmpeg that counts its runs and copies `image` to its last argument.
fn fake_ffmpeg(dir: &Path, image: Option<&Path>) -> PathBuf {
    let script = dir.join("ffmpeg");
    let counter = dir.join("runs");
    let body = match image {
        Some(img) => format!(
            "#!/bin/sh\necho run >> '{}'\nfor a; do out=$a; done\nsleep 0.2\ncp '{}' \"$out\"\n",
            counter.display(),
            img.display()
        ),
        None => format!("#!/bin/sh\necho run >> '{}'\nexit 1\n", counter.display()),
    };
    std::fs::write(&script, body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

fn runs(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("runs"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_frame_is_grabbed_once_and_cached() {
    let d = dir("ok");
    let png = Placeholder::Landscape.ensure_file(&d.join("src")).unwrap();
    let ffmpeg = fake_ffmpeg(&d, Some(&png));
    let video = d.join("Show - S01E01.mkv");
    std::fs::write(&video, b"").unwrap();
    let cache = d.join("cache");

    let mut tasks = Vec::new();
    for _ in 0..6 {
        let (ffmpeg, video, cache) = (ffmpeg.clone(), video.clone(), cache.clone());
        tasks.push(tokio::spawn(async move {
            image_processing::video_frame(&ffmpeg.to_string_lossy(), &video, &cache, "k1", 42.0)
                .await
                .map_err(|e| e.to_string())
        }));
    }
    let mut paths = Vec::new();
    for t in tasks {
        paths.push(t.await.unwrap().expect("frame"));
    }
    assert_eq!(runs(&d), 1, "one ffmpeg run for six requests");
    assert!(paths.windows(2).all(|w| w[0] == w[1]));
    assert!(paths[0].starts_with(&cache));
    assert!(std::fs::read(&paths[0]).unwrap().starts_with(b"\x89PNG"));

    // Cached: no new run.
    image_processing::video_frame(&ffmpeg.to_string_lossy(), &video, &cache, "k1", 42.0)
        .await
        .unwrap();
    assert_eq!(runs(&d), 1);
    std::fs::remove_dir_all(&d).unwrap();
}

#[tokio::test]
async fn a_failed_grab_is_an_error_and_leaves_nothing() {
    let d = dir("fail");
    let ffmpeg = fake_ffmpeg(&d, None);
    let video = d.join("Show - S01E01.mkv");
    std::fs::write(&video, b"").unwrap();
    let cache = d.join("cache");

    let got =
        image_processing::video_frame(&ffmpeg.to_string_lossy(), &video, &cache, "k2", 42.0).await;
    assert!(got.is_err());
    let left: Vec<_> = std::fs::read_dir(cache.join("frames"))
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
    std::fs::remove_dir_all(&d).unwrap();
}
