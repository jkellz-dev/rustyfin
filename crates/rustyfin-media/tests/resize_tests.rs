//! Many requests for the same thumbnail resize once, and nobody sees a
//! half-written file.

use rustyfin_media::image_processing::{self, ImageFormat, ImageOptions, Placeholder};
use std::path::{Path, PathBuf};

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_for_one_thumbnail_resize_once() {
    let dir = std::env::temp_dir().join(format!("rustyfin-resize-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = Placeholder::Poster
        .ensure_file(&dir.join("src"))
        .expect("png");
    let cache = dir.join("cache");
    let opts = ImageOptions {
        max_width: Some(64),
        max_height: None,
        width: None,
        height: None,
        quality: 80,
        format: ImageFormat::Jpeg,
    };

    let mut tasks = Vec::new();
    for _ in 0..12 {
        let (src, cache, opts) = (src.clone(), cache.clone(), opts.clone());
        tasks.push(tokio::spawn(async move {
            image_processing::process_image_traced(&src.to_string_lossy(), &opts, &cache)
                .await
                .map_err(|e| e.to_string())
        }));
    }
    let mut paths = Vec::new();
    let mut resized = 0;
    for t in tasks {
        let (path, did_work) = t.await.unwrap().expect("processed");
        paths.push(path);
        resized += usize::from(did_work);
    }
    assert_eq!(resized, 1, "one resize for twelve identical requests");
    assert!(paths.windows(2).all(|w| w[0] == w[1]));
    let left = files_under(&cache);
    assert_eq!(
        left,
        vec![paths[0].clone()],
        "no temporary files left: {left:?}"
    );
    let bytes = std::fs::read(&paths[0]).unwrap();
    assert!(bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "a complete JPEG");
    std::fs::remove_dir_all(&dir).unwrap();
}
