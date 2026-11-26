//! Many requests for the same thumbnail resize once, and nobody sees a
//! half-written file; a resize keeps what the picture needs (transparency).

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

fn resize_original(src: &Path, cache: &Path) -> Vec<u8> {
    let opts = ImageOptions {
        max_width: Some(40),
        max_height: None,
        width: None,
        height: None,
        quality: 90,
        format: ImageFormat::Original,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let out = rt
        .block_on(image_processing::process_image(
            &src.to_string_lossy(),
            &opts,
            cache,
        ))
        .expect("processed");
    std::fs::read(out).unwrap()
}

/// A title logo is a PNG with a transparent background. Apps ask for it at a
/// width without naming a format (Swiftfin: `maxWidth=`); the answer must
/// stay transparent, as Jellyfin's does, not gain a black box.
#[test]
fn a_transparent_picture_stays_transparent_when_resized() {
    let dir = std::env::temp_dir().join(format!("rustyfin-alpha-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Clear everywhere but an opaque white bar across the middle.
    let logo = image::RgbaImage::from_fn(80, 40, |_, y| {
        if (15..25).contains(&y) {
            image::Rgba([255, 255, 255, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    let src = dir.join("clearlogo.png");
    logo.save(&src).unwrap();

    let bytes = resize_original(&src, &dir.join("cache"));
    let out = image::load_from_memory(&bytes).expect("an image");
    assert!(
        out.color().has_alpha(),
        "the resized logo keeps its alpha channel"
    );
    let out = out.to_rgba8();
    assert_eq!(out.width(), 40);
    assert_eq!(out.get_pixel(20, 1)[3], 0, "the clear corner stays clear");
    assert_eq!(out.get_pixel(20, 10)[3], 255, "the bar stays opaque");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// An opaque picture (a poster, even one saved as PNG) is still sent as a
/// JPEG when resized: smaller, and nothing is lost.
#[test]
fn an_opaque_picture_is_resized_to_a_jpeg() {
    let dir = std::env::temp_dir().join(format!("rustyfin-opaque-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let poster = image::RgbImage::from_fn(80, 120, |x, y| image::Rgb([x as u8, y as u8, 90]));
    let src = dir.join("poster.png");
    poster.save(&src).unwrap();
    let bytes = resize_original(&src, &dir.join("cache"));
    assert!(bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "a JPEG");

    // An alpha channel that is opaque everywhere is no transparency either.
    let rgba = image::RgbaImage::from_fn(80, 120, |x, y| image::Rgba([x as u8, y as u8, 90, 255]));
    let src = dir.join("poster-rgba.png");
    rgba.save(&src).unwrap();
    let bytes = resize_original(&src, &dir.join("cache"));
    assert!(bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "a JPEG");
    std::fs::remove_dir_all(&dir).unwrap();
}
