use std::path::Path;

use crate::library::naming::is_image_file;

#[derive(Debug, Clone)]
pub struct DiscoveredImage {
    pub image_type: String,
    pub path: String,
    pub image_index: i32,
}

/// Discover sidecar artwork files in a directory for a given media item.
///
/// `item_stem` is the filename stem of the video file (e.g., "Movie Name (2020)").
pub fn discover_images(dir: &Path, item_stem: &str) -> Vec<DiscoveredImage> {
    let mut images = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return images;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_image_file(&path) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem_lower = stem.to_lowercase();
        let path_str = path.to_string_lossy().to_string();

        if let Some(img) = classify_image(&stem_lower, item_stem, &path_str) {
            images.push(img);
        }
    }

    images.sort_by(|a, b| {
        a.image_type
            .cmp(&b.image_type)
            .then(a.image_index.cmp(&b.image_index))
    });
    images
}

fn classify_image(stem_lower: &str, item_stem: &str, path: &str) -> Option<DiscoveredImage> {
    // Primary / Poster
    if matches!(
        stem_lower,
        "poster" | "folder" | "cover" | "default" | "movie" | "show"
    ) || stem_lower == item_stem.to_lowercase()
    {
        return Some(DiscoveredImage {
            image_type: "Primary".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }

    // Backdrop / Fanart (with optional numbering)
    if stem_lower == "fanart" || stem_lower == "backdrop" || stem_lower == "background" {
        return Some(DiscoveredImage {
            image_type: "Backdrop".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }
    if let Some(rest) = stem_lower.strip_prefix("fanart")
        && let Ok(n) = rest.parse::<i32>()
    {
        return Some(DiscoveredImage {
            image_type: "Backdrop".to_string(),
            path: path.to_string(),
            image_index: n.saturating_sub(1),
        });
    }
    if let Some(rest) = stem_lower.strip_prefix("backdrop")
        && let Ok(n) = rest.parse::<i32>()
    {
        return Some(DiscoveredImage {
            image_type: "Backdrop".to_string(),
            path: path.to_string(),
            image_index: n.saturating_sub(1),
        });
    }

    // Banner
    if stem_lower == "banner" {
        return Some(DiscoveredImage {
            image_type: "Banner".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }

    // Logo
    if stem_lower == "logo" || stem_lower == "clearlogo" {
        return Some(DiscoveredImage {
            image_type: "Logo".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }

    // Thumb
    if stem_lower == "thumb" || stem_lower == "landscape" {
        return Some(DiscoveredImage {
            image_type: "Thumb".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }

    None
}
