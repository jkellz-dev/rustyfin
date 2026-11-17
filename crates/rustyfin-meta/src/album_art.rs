//! `rustyfin artwork -t music`: album covers from the Cover Art Archive for
//! albums with no cover file (owner, 2026-11-16: new Lidarr albums had no
//! picture anywhere). Albums are found by the MusicBrainz ids Lidarr and
//! Picard already write (`album.nfo`, else the first track's tags), never by
//! name. The cover is saved as `cover.jpg` in the album folder, which the
//! server picks up at its next scan. Existing covers are never replaced
//! (`--force` does). Albums the archive has no cover for record
//! `[artwork] missing_checked` in their `.rustyfin.toml` and are not asked
//! again for 30 days.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::images::discover_images;
use rustyfin_common::naming::is_audio_file;
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::{music_tags, nfo, title_settings};

use crate::scan::ScanOpts;

/// An album the archive had no cover for is asked again after this long.
const RECHECK_AFTER: Duration = Duration::from_secs(30 * 24 * 3600);

/// The archive's 1200 px thumbnail: always a JPEG, a few hundred KB, unlike
/// the originals (often PNG or TIFF scans of many MB).
const SIZE: &str = "front-1200";

/// What a music artwork run did.
#[derive(Debug, Default)]
pub struct AlbumArtStats {
    pub albums: usize,
    pub had_cover: usize,
    pub downloaded: usize,
    pub no_id: usize,
    pub not_found: usize,
    pub recently_checked: usize,
    pub errors: usize,
}

/// The folders under `root` (itself included) that hold audio files
/// directly: the albums, in path order.
pub fn album_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && is_audio_file(e.path()))
        .filter_map(|e| e.path().parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Whether the album folder already has a cover the server would use.
pub fn has_cover(dir: &Path) -> bool {
    discover_images(dir, "")
        .iter()
        .any(|i| i.image_type == "Primary")
}

/// The album's MusicBrainz release and release-group ids: `album.nfo`
/// (Lidarr) first, else the first track's tags (Picard, Lidarr).
pub fn album_mbids(dir: &Path) -> (Option<String>, Option<String>) {
    let clean = |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if let Some(meta) = nfo::find_album_nfo(dir).and_then(|p| nfo::parse_album_nfo(&p)) {
        let ids = (
            clean(meta.musicbrainz_album_id),
            clean(meta.musicbrainz_release_group_id),
        );
        if ids.0.is_some() || ids.1.is_some() {
            return ids;
        }
    }
    let mut tracks: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_audio_file(p))
        .collect();
    tracks.sort();
    tracks
        .iter()
        .find_map(|t| {
            let tags = music_tags::read_tags(t)?;
            let ids = (
                clean(tags.musicbrainz_album_id),
                clean(tags.musicbrainz_release_group_id),
            );
            (ids.0.is_some() || ids.1.is_some()).then_some(ids)
        })
        .unwrap_or_default()
}

/// The archive addresses to try, best first: the exact release, then any
/// release of the release group.
pub fn cover_urls(base: &str, release: Option<&str>, group: Option<&str>) -> Vec<String> {
    let base = base.trim_end_matches('/');
    let safe = |id: &str| id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let mut urls = Vec::new();
    if let Some(r) = release.filter(|r| safe(r)) {
        urls.push(format!("{base}/release/{r}/{SIZE}"));
    }
    if let Some(g) = group.filter(|g| safe(g)) {
        urls.push(format!("{base}/release-group/{g}/{SIZE}"));
    }
    urls
}

/// Fetch missing album covers for every album under `root`.
pub async fn fill_album_art(root: &Path, opts: &ScanOpts<'_>) -> anyhow::Result<AlbumArtStats> {
    if !root.is_dir() {
        anyhow::bail!("cannot read library folder {}", root.display());
    }
    let cfg = &opts.config.coverart;
    let limiter = RateLimiter::new(cfg.rate_limit, cfg.rate_window_secs);
    let http = reqwest::Client::builder()
        .user_agent(concat!(
            "Rustyfin/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/jkellz-dev/rustyfin)"
        ))
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut stats = AlbumArtStats::default();
    for dir in album_dirs(root) {
        stats.albums += 1;
        let name = dir.strip_prefix(root).unwrap_or(&dir).display().to_string();
        if has_cover(&dir) && !opts.force {
            stats.had_cover += 1;
            continue;
        }
        let settings = title_settings::load(&dir);
        if settings.as_ref().is_some_and(|s| s.artwork.skip) {
            continue;
        }
        if !opts.force
            && settings
                .as_ref()
                .is_some_and(|s| s.missing_checked_recently(RECHECK_AFTER))
        {
            stats.recently_checked += 1;
            continue;
        }
        let (release, group) = album_mbids(&dir);
        let urls = cover_urls(&cfg.url, release.as_deref(), group.as_deref());
        if urls.is_empty() {
            tracing::debug!("{name}: no MusicBrainz id");
            stats.no_id += 1;
            continue;
        }
        if opts.dry_run {
            tracing::info!("{name}: would fetch {}", urls[0]);
            continue;
        }
        let target = dir.join("cover.jpg");
        let mut saved = false;
        let mut failed = false;
        for url in &urls {
            limiter.wait().await;
            match download_image_with_options(&http, url, &target, !opts.force).await {
                Ok(()) => {
                    saved = true;
                    break;
                }
                Err(e) if e.to_string().contains("HTTP 404") => {}
                Err(e) => {
                    tracing::error!("{name}: {url}: {e}");
                    failed = true;
                }
            }
        }
        if saved {
            tracing::info!("{name}: saved {}", target.display());
            stats.downloaded += 1;
        } else if failed {
            stats.errors += 1;
        } else {
            stats.not_found += 1;
            if let Err(e) = title_settings::record_missing_checked_now(&dir) {
                tracing::warn!("{name}: cannot record that the archive has no cover: {e}");
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "rustyfin-albumart-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn albums_are_the_folders_that_hold_tracks() {
        let t = Tree::new();
        t.file("Wavves/Cherry Soda (2026)/01 We Get Around.flac", "");
        t.file("Wavves/Cherry Soda (2026)/02 Pinesol.flac", "");
        t.file("Wavves/artist.nfo", "<artist/>");
        t.file("Jeff Beck/Live in Tokyo (2014)/CD1/01 Loaded.mp3", "");
        let dirs = album_dirs(&t.0);
        assert_eq!(
            dirs,
            [
                t.0.join("Jeff Beck/Live in Tokyo (2014)/CD1"),
                t.0.join("Wavves/Cherry Soda (2026)"),
            ]
        );
    }

    #[test]
    fn a_folder_with_a_cover_file_needs_none() {
        let t = Tree::new();
        t.file("A/B/01.flac", "");
        assert!(!has_cover(&t.0.join("A/B")));
        t.file("A/B/folder.jpg", "x");
        assert!(has_cover(&t.0.join("A/B")));
    }

    #[test]
    fn lidarrs_album_nfo_names_the_release() {
        let t = Tree::new();
        t.file("A/B/01.flac", "");
        t.file(
            "A/B/album.nfo",
            "<album><title>Cherry Soda</title>\
             <musicbrainzalbumid>d56aacad-e477-4921-b858-40bda0d8b5e3</musicbrainzalbumid>\
             <musicbrainzreleasegroupid>28b5d5e5-3c90-41c6-a736-21effb7d9e5d</musicbrainzreleasegroupid>\
             </album>",
        );
        assert_eq!(
            album_mbids(&t.0.join("A/B")),
            (
                Some("d56aacad-e477-4921-b858-40bda0d8b5e3".to_string()),
                Some("28b5d5e5-3c90-41c6-a736-21effb7d9e5d".to_string())
            )
        );
        t.file("C/D/01.flac", "");
        assert_eq!(album_mbids(&t.0.join("C/D")), (None, None));
    }

    #[test]
    fn the_release_is_tried_before_its_group() {
        assert_eq!(
            cover_urls(
                "https://coverartarchive.org/",
                Some("d56aacad-e477"),
                Some("28b5d5e5-3c90")
            ),
            [
                "https://coverartarchive.org/release/d56aacad-e477/front-1200",
                "https://coverartarchive.org/release-group/28b5d5e5-3c90/front-1200"
            ]
        );
        assert_eq!(
            cover_urls("https://c", None, Some("ab-12")),
            ["https://c/release-group/ab-12/front-1200"]
        );
        // Anything that is not an id is never put in a URL.
        assert!(cover_urls("https://c", Some("../x?y"), None).is_empty());
        assert!(cover_urls("https://c", None, None).is_empty());
    }
}
