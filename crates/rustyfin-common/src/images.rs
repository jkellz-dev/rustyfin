use std::path::Path;

use crate::naming::is_image_file;

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
        "poster" | "folder" | "cover" | "default" | "movie" | "show" | "artist" | "front"
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

    // Disc art (common in music libraries)
    if stem_lower == "disc" || stem_lower == "cdart" || stem_lower == "discart" {
        return Some(DiscoveredImage {
            image_type: "Disc".to_string(),
            path: path.to_string(),
            image_index: 0,
        });
    }

    None
}

/// The file-name stems a season's art uses in the show folder (Kodi, Sonarr and
/// Jellyfin): `season01`, `season1` and `season-specials` for season 0.
fn season_prefixes(season: i32) -> Vec<String> {
    if season == 0 {
        vec![
            "season-specials".to_string(),
            "season00".to_string(),
            "season0".to_string(),
        ]
    } else {
        vec![format!("season{season:02}"), format!("season{season}")]
    }
}

/// A season's art kept in the show folder: `season01-poster.jpg` (or plain
/// `season01.jpg`), `-banner`, `-fanart`, `-landscape`/`-thumb`. This is where
/// Sonarr's Kodi metadata and `rustyfin fetch` write it.
pub fn discover_season_images(series_dir: &Path, season: i32) -> Vec<DiscoveredImage> {
    let prefixes = season_prefixes(season);
    let mut images = Vec::new();
    let Ok(entries) = std::fs::read_dir(series_dir) else {
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
        let stem = stem.to_lowercase();
        let Some(rest) = prefixes.iter().find_map(|p| stem.strip_prefix(p.as_str())) else {
            continue;
        };
        let image_type = match rest {
            "" | "-poster" => "Primary",
            "-banner" => "Banner",
            "-fanart" | "-backdrop" => "Backdrop",
            "-landscape" | "-thumb" => "Thumb",
            _ => continue,
        };
        if images
            .iter()
            .any(|i: &DiscoveredImage| i.image_type == image_type)
        {
            continue;
        }
        images.push(DiscoveredImage {
            image_type: image_type.to_string(),
            path: path.to_string_lossy().to_string(),
            image_index: 0,
        });
    }
    images.sort_by(|a, b| a.image_type.cmp(&b.image_type));
    images
}

/// An episode's own images: files named after the episode, `<episode>-thumb.jpg`
/// (Sonarr/Kodi) or `<episode>.jpg`, as its primary image. The season's
/// `poster.jpg` in the same folder belongs to the season, not to each episode.
pub fn discover_episode_images(dir: &Path, episode_stem: &str) -> Vec<DiscoveredImage> {
    let stem = episode_stem.to_lowercase();
    let mut found: Option<(u8, String)> = None;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_image_file(&path) {
            continue;
        }
        let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let file_stem = file_stem.to_lowercase();
        // `-thumb` wins over a bare same-name image.
        let rank = if file_stem == format!("{stem}-thumb") {
            0
        } else if file_stem == stem {
            1
        } else {
            continue;
        };
        if found.as_ref().is_none_or(|(r, _)| rank < *r) {
            found = Some((rank, path.to_string_lossy().to_string()));
        }
    }
    found
        .map(|(_, path)| {
            vec![DiscoveredImage {
                image_type: "Primary".to_string(),
                path,
                image_index: 0,
            }]
        })
        .unwrap_or_default()
}

/// The file a person's headshot is kept in, inside the shared people folder.
/// Never the person's name (owner, 2026-11-14: names are not identities):
/// `tmdb-<id>.jpg` when their TMDB id is known (`tmdb-<digits>`, as stored),
/// else `img-<sha256>.jpg` of the photo's address, so one photo is one file
/// however many people or films point at it. A TMDB photo's address is its path
/// without the size (`/kSjVX….jpg`), so `original` and `h632` links agree. `None`
/// when there is neither.
pub fn headshot_file_name(
    provider_person_id: Option<&str>,
    image_url: Option<&str>,
) -> Option<String> {
    if let Some(id) = provider_person_id.and_then(|p| p.strip_prefix("tmdb-"))
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(format!("tmdb-{id}.jpg"));
    }
    let url = image_url.map(str::trim).filter(|u| !u.is_empty())?;
    const TMDB: &str = "image.tmdb.org/t/p/";
    let address = match url.find(TMDB) {
        Some(at) => {
            let rest = &url[at + TMDB.len()..];
            match rest.find('/') {
                Some(slash) => format!("tmdb:{}", &rest[slash..]),
                None => url.to_string(),
            }
        }
        None => url.to_string(),
    };
    use sha2::Digest;
    let digest = sha2::Sha256::digest(address.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("img-{hex}.jpg"))
}

#[cfg(test)]
mod season_image_tests {
    use super::*;

    #[test]
    fn season_ten_is_not_season_one() {
        let dir = std::env::temp_dir().join(format!("rf-season-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "season10-poster.jpg",
            "season1-banner.jpg",
            "season-specials-poster.png",
        ] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let types = |n| {
            discover_season_images(&dir, n)
                .into_iter()
                .map(|i| i.image_type)
                .collect::<Vec<_>>()
        };
        assert_eq!(types(1), vec!["Banner"]);
        assert_eq!(types(10), vec!["Primary"]);
        assert_eq!(types(0), vec!["Primary"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
