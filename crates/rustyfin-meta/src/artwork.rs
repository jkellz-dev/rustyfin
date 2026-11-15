//! `rustyfin artwork`: fill in missing artwork from TMDB.
//!
//! For libraries another tool (Sonarr, Radarr) already manages: NFOs and
//! existing images are never touched. Each show or film folder is checked for a
//! poster and fanart, and each season folder for a cover (in the season folder
//! or as `season01-poster.jpg` in the show folder). Only folders missing
//! something are looked up, by the id already known: `[tmdbid-N]` in the folder
//! name or the NFO, else the `[tvdbid-N]` / IMDb id through TMDB's find. There
//! is no title search, so nothing can be matched to the wrong show. Downloads go
//! where the server reads them: `poster.jpg`, `fanart.jpg`, `season01-poster.jpg`,
//! `season-specials-poster.jpg`.

use std::path::{Path, PathBuf};

use rustyfin_common::artwork::download_image_with_options;
use rustyfin_common::images::{discover_images, discover_season_images};
use rustyfin_common::naming::{self, is_ignored_name, parse_season_folder};
use rustyfin_common::nfo;
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::tmdb::TmdbClient;

use crate::scan::ScanOpts;

/// What a show or film folder lacks.
#[derive(Debug, Default, PartialEq)]
pub struct Missing {
    pub poster: bool,
    pub fanart: bool,
    /// Seasons (by number) without a cover.
    pub seasons: Vec<i32>,
}

impl Missing {
    pub fn is_empty(&self) -> bool {
        !self.poster && !self.fanart && self.seasons.is_empty()
    }
}

/// The file a season's poster is saved as, in the show folder.
pub fn season_poster_name(season: i32) -> String {
    if season == 0 {
        "season-specials-poster.jpg".to_string()
    } else {
        format!("season{season:02}-poster.jpg")
    }
}

fn has_type(dir: &Path, image_type: &str) -> bool {
    discover_images(dir, "")
        .iter()
        .any(|i| i.image_type == image_type)
}

/// What `dir` lacks; season folders are checked when `tv`.
pub fn missing_art(dir: &Path, tv: bool) -> Missing {
    let mut missing = Missing {
        poster: !has_type(dir, "Primary"),
        fanart: !has_type(dir, "Backdrop"),
        seasons: Vec::new(),
    };
    if tv && let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !path.is_dir() || is_ignored_name(name) {
                continue;
            }
            let Some(n) = parse_season_folder(name) else {
                continue;
            };
            let in_show_folder = discover_season_images(dir, n)
                .iter()
                .any(|i| i.image_type == "Primary");
            if !in_show_folder && !has_type(&path, "Primary") {
                missing.seasons.push(n);
            }
        }
        missing.seasons.sort();
        missing.seasons.dedup();
    }
    missing
}

/// The ids known for a folder without asking anyone: folder name, then NFO.
fn known_ids(dir: &Path, tv: bool) -> (Option<i64>, Option<String>, Option<String>) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let mut tmdb = None;
    let mut tvdb = None;
    let mut imdb = None;
    if let Some(pid) = naming::extract_provider_id(name) {
        match pid.provider.as_str() {
            "tmdb" => tmdb = pid.id.parse().ok(),
            "tvdb" => tvdb = Some(pid.id),
            "imdb" => imdb = Some(pid.id),
            _ => {}
        }
    }
    let meta = if tv {
        nfo::find_series_nfo(dir).and_then(|p| nfo::parse_series_nfo(&p))
    } else {
        first_video(dir)
            .and_then(|v| nfo::find_movie_nfo(&v))
            .and_then(|p| nfo::parse_movie_nfo(&p))
    };
    if let Some(meta) = meta {
        tmdb = tmdb.or_else(|| meta.tmdb_id.and_then(|id| id.parse().ok()));
        tvdb = tvdb.or(meta.tvdb_id);
        imdb = imdb.or(meta.imdb_id);
    }
    (tmdb, tvdb, imdb)
}

fn first_video(dir: &Path) -> Option<PathBuf> {
    let mut videos: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| naming::is_video_file(p))
        .collect();
    videos.sort();
    videos.into_iter().next()
}

#[derive(Debug, Default)]
pub struct ArtworkStats {
    pub checked: usize,
    pub complete: usize,
    pub downloaded: usize,
    pub headshots: usize,
    pub no_id: usize,
    pub errors: usize,
}

/// TMDB's size for headshots: 632 px tall, about 50 KB.
pub const HEADSHOT_SIZE: &str = "h632";

/// The headshots to download into the shared `people_dir`: one per file (TMDB
/// id, else the photo's address; see `headshot_file_name`) that has an image URL
/// and does not exist yet, so a person or photo listed twice is fetched once.
pub fn headshots_wanted(people: &[nfo::NfoPerson], people_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut seen = std::collections::HashSet::new();
    let mut wanted = Vec::new();
    for person in people {
        let url = person.image_url.as_deref().filter(|u| !u.trim().is_empty());
        let (Some(url), Some(file)) = (
            url,
            rustyfin_common::images::headshot_file_name(person.provider_person_id.as_deref(), url),
        ) else {
            continue;
        };
        if !seen.insert(file.clone()) {
            continue;
        }
        let path = people_dir.join(file);
        if !path.exists() {
            wanted.push((
                rustyfin_common::artwork::resized_tmdb_image_url(url, HEADSHOT_SIZE),
                path,
            ));
        }
    }
    wanted
}

/// The cast and crew of a show or film folder from its NFO (Radarr and Sonarr
/// write actor thumbs there).
fn nfo_people(dir: &Path, tv: bool) -> Vec<nfo::NfoPerson> {
    let meta = if tv {
        nfo::find_series_nfo(dir).and_then(|p| nfo::parse_series_nfo(&p))
    } else {
        first_video(dir)
            .and_then(|v| nfo::find_movie_nfo(&v))
            .and_then(|p| nfo::parse_movie_nfo(&p))
    };
    meta.map(|m| m.people).unwrap_or_default()
}

/// Fill missing artwork for every show (`tvshows`) or film (`movies`) folder in `root`.
pub async fn fill_missing(
    root: &Path,
    library_type: &str,
    opts: &ScanOpts<'_>,
) -> anyhow::Result<ArtworkStats> {
    let tv = match library_type {
        "tvshows" | "tv" | "series" => true,
        "movies" | "movie" => false,
        other => anyhow::bail!("Unknown library type: {other}. Use: movies, tvshows"),
    };
    let tmdb_key = opts.config.tmdb.api_key.clone();
    if tmdb_key.is_none() && !opts.dry_run {
        anyhow::bail!(
            "A TMDB credential is required: set [tmdb] api_key (or RUSTYFIN__TMDB__API_KEY) \
             to the API Read Access Token or a v3 API key"
        );
    }
    let tmdb = tmdb_key.map(TmdbClient::new);
    let limiter = RateLimiter::new(
        opts.config.tmdb.rate_limit,
        opts.config.tmdb.rate_window_secs,
    );
    let http = reqwest::Client::new();
    let skip_existing = !opts.force;
    let people_dir = PathBuf::from(crate::scan::expand_tilde(
        &opts.config.output.people_cache_dir,
    ));
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !is_ignored_name(n))
        })
        .collect();
    dirs.sort();

    let mut stats = ArtworkStats::default();
    for dir in dirs {
        stats.checked += 1;
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // Headshots: the NFO's cast first (no lookup needed), else TMDB's
        // credits below. Shared by everyone the person appears with.
        let local_people = nfo_people(&dir, tv);
        let mut headshots = headshots_wanted(&local_people, &people_dir);
        if !headshots.is_empty() {
            if opts.dry_run {
                tracing::info!("{name}: {} headshots to fetch", headshots.len());
            } else {
                stats.headshots +=
                    fetch_headshots(&http, &headshots, &name, &mut stats.errors).await;
            }
            headshots.clear();
        }
        let need_credits = !local_people.iter().any(|p| p.image_url.is_some());
        let missing = missing_art(&dir, tv);
        if missing.is_empty() && !opts.force && !need_credits {
            stats.complete += 1;
            continue;
        }
        if opts.dry_run {
            if !missing.is_empty() {
                tracing::info!("{name}: missing {missing:?}");
            }
            continue;
        }
        if !missing.is_empty() {
            tracing::info!("{name}: missing {missing:?}");
        }
        let Some(tmdb) = tmdb.as_ref() else { continue };

        let (tmdb_id, tvdb_id, imdb_id) = known_ids(&dir, tv);
        let mut id = tmdb_id;
        if id.is_none()
            && tv
            && let Some(tvdb) = &tvdb_id
        {
            limiter.wait().await;
            id = tmdb.find_by_external_id("tvdb_id", tvdb, true).await;
        }
        if id.is_none()
            && let Some(imdb) = &imdb_id
        {
            limiter.wait().await;
            id = tmdb.find_by_external_id("imdb_id", imdb, tv).await;
        }
        let Some(id) = id else {
            tracing::warn!("{name}: no TMDB, TVDB or IMDb id in the folder name or NFO; skipped");
            stats.no_id += 1;
            continue;
        };
        limiter.wait().await;
        if need_credits {
            limiter.wait().await;
            let details = if tv {
                tmdb.fetch_series_by_id(id).await
            } else {
                tmdb.fetch_movie_by_id(id).await
            };
            if let Some(details) = details {
                headshots = headshots_wanted(&details.people, &people_dir);
                stats.headshots +=
                    fetch_headshots(&http, &headshots, &name, &mut stats.errors).await;
            }
            if missing.is_empty() && !opts.force {
                stats.complete += 1;
                continue;
            }
        }
        limiter.wait().await;
        let Some(art) = tmdb.fetch_artwork(id, tv).await else {
            tracing::error!("{name}: TMDB {id} did not answer");
            stats.errors += 1;
            continue;
        };

        let mut wanted: Vec<(String, PathBuf)> = Vec::new();
        if missing.poster || opts.force {
            wanted.extend(art.poster.clone().map(|u| (u, dir.join("poster.jpg"))));
        }
        if missing.fanart || opts.force {
            wanted.extend(art.backdrop.clone().map(|u| (u, dir.join("fanart.jpg"))));
        }
        for (n, url) in &art.season_posters {
            if missing.seasons.contains(n) || (opts.force && tv) {
                wanted.push((url.clone(), dir.join(season_poster_name(*n))));
            }
        }
        for (url, path) in wanted {
            match download_image_with_options(&http, &url, &path, skip_existing).await {
                Ok(()) => {
                    tracing::info!("{name}: saved {}", path.display());
                    stats.downloaded += 1;
                }
                Err(e) => {
                    tracing::error!("{name}: {}: {e}", path.display());
                    stats.errors += 1;
                }
            }
        }
    }
    Ok(stats)
}

/// Download `wanted` headshots; returns how many were saved.
async fn fetch_headshots(
    http: &reqwest::Client,
    wanted: &[(String, PathBuf)],
    name: &str,
    errors: &mut usize,
) -> usize {
    if let Some(dir) = wanted.first().and_then(|(_, p)| p.parent())
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::error!("{name}: cannot create {}: {e}", dir.display());
        *errors += 1;
        return 0;
    }
    let mut saved = 0;
    for (url, path) in wanted {
        match download_image_with_options(http, url, path, true).await {
            Ok(()) => saved += 1,
            Err(e) => {
                tracing::warn!("{name}: headshot {}: {e}", path.display());
                *errors += 1;
            }
        }
    }
    if saved > 0 {
        tracing::info!("{name}: saved {saved} headshots");
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "rustyfin-art-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, rel: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn collections_without_a_poster_need_art_once_each() {
        let t = Tree::new();
        let nfo = |id: &str| {
            format!(r#"<movie><title>x</title><set tmdbcolid="{id}"><name>S</name></set></movie>"#)
        };
        for (film, set) in [("A (2001)", "10"), ("B (2002)", "10"), ("C (2003)", "20")] {
            t.file(&format!("movies/{film}/{film}.mkv"));
            std::fs::write(t.0.join(format!("movies/{film}/{film}.nfo")), nfo(set)).unwrap();
        }
        t.file("movies/D (2004)/D (2004).mkv");
        t.file("collections/20/poster.jpg");
        let dirs: Vec<PathBuf> = ["A (2001)", "B (2002)", "C (2003)", "D (2004)"]
            .iter()
            .map(|d| t.0.join("movies").join(d))
            .collect();
        let collections = t.0.join("collections");
        assert_eq!(collections_needing_art(&dirs, &collections, false), [10]);
        assert_eq!(collections_needing_art(&dirs, &collections, true), [10, 20]);
    }

    #[test]
    fn collection_art_sits_beside_the_people_folder() {
        assert_eq!(
            rustyfin_common::images::collection_art_dir(Path::new("/data/cache/people")),
            Path::new("/data/cache/collections")
        );
    }

    #[tokio::test]
    async fn a_missing_library_folder_is_named_in_the_error() {
        let config = crate::config::MetaConfig::default();
        let opts = crate::scan::ScanOpts {
            config: &config,
            dry_run: true,
            auto_mode: false,
            no_cache: true,
            force: false,
        };
        let err = fill_missing(Path::new("/data/media/kids_movies"), "movies", &opts)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("/data/media/kids_movies"), "{err}");
    }

    fn person(name: &str, url: Option<&str>) -> nfo::NfoPerson {
        nfo::NfoPerson {
            name: name.to_string(),
            person_type: "Actor".to_string(),
            role: None,
            sort_order: None,
            image_url: url.map(str::to_string),
            provider_person_id: None,
        }
    }

    fn file(id: Option<&str>, url: Option<&str>) -> String {
        rustyfin_common::images::headshot_file_name(id, url).unwrap()
    }

    #[test]
    fn headshots_are_fetched_once_per_person_into_the_shared_folder() {
        let t = Tree::new();
        // Already fetched for another film.
        t.file(&format!("people/{}", file(None, Some("https://img/a.jpg"))));
        let mut paul = person("Aaron Paul", Some("https://img/b.jpg"));
        paul.provider_person_id = Some("tmdb-84497".to_string());
        let mut paul_again = person("Aaron Paul", Some("https://img/c.jpg"));
        paul_again.provider_person_id = Some("tmdb-84497".to_string());
        let people = [
            person("Bryan Cranston", Some("https://img/a.jpg")),
            paul,
            paul_again,
            person("Same Photo", Some("https://img/d.jpg")),
            person("Same Photo Again", Some("https://img/d.jpg")),
            person("No Photo", None),
        ];
        assert_eq!(
            headshots_wanted(&people, &t.0.join("people")),
            vec![
                (
                    "https://img/b.jpg".to_string(),
                    t.0.join("people/tmdb-84497.jpg")
                ),
                (
                    "https://img/d.jpg".to_string(),
                    t.0.join("people")
                        .join(file(None, Some("https://img/d.jpg")))
                ),
            ]
        );
    }

    #[test]
    fn tmdb_headshots_are_fetched_at_632_px_not_the_original() {
        let t = Tree::new();
        let people = [
            person(
                "Bryan Cranston",
                Some("http://image.tmdb.org/t/p/original/cjeDbVfBp6Qvb3C74Dfy7BKDTQN.jpg"),
            ),
            person("Aaron Paul", Some("https://artworks.thetvdb.com/a/b.jpg")),
        ];
        let urls: Vec<String> = headshots_wanted(&people, &t.0)
            .into_iter()
            .map(|(u, _)| u)
            .collect();
        assert_eq!(
            urls,
            vec![
                "https://image.tmdb.org/t/p/h632/cjeDbVfBp6Qvb3C74Dfy7BKDTQN.jpg".to_string(),
                "https://artworks.thetvdb.com/a/b.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn season_posters_are_named_as_the_server_reads_them() {
        assert_eq!(season_poster_name(0), "season-specials-poster.jpg");
        assert_eq!(season_poster_name(1), "season01-poster.jpg");
        assert_eq!(season_poster_name(12), "season12-poster.jpg");
    }

    #[test]
    fn a_show_lacking_season_covers_says_which() {
        let t = Tree::new();
        t.file("Show/poster.jpg");
        t.file("Show/fanart.jpg");
        t.file("Show/season01-poster.jpg");
        t.file("Show/Season 01/e1.mkv");
        t.file("Show/Season 02/folder.jpg");
        t.file("Show/Season 02/e1.mkv");
        t.file("Show/Season 03/e1.mkv");
        t.file("Show/Specials/e1.mkv");
        t.file("Show/.Trash-1000/e1.mkv");
        t.file("Show/extras-not-a-season/x.mkv");
        let m = missing_art(&t.0.join("Show"), true);
        assert_eq!(
            m,
            Missing {
                poster: false,
                fanart: false,
                seasons: vec![0, 3]
            }
        );
    }

    #[test]
    fn a_complete_folder_needs_nothing() {
        let t = Tree::new();
        t.file("Film (2020)/Film (2020).mkv");
        t.file("Film (2020)/folder.jpg");
        t.file("Film (2020)/backdrop.png");
        assert!(missing_art(&t.0.join("Film (2020)"), false).is_empty());
        t.file("Bare (2021)/Bare (2021).mkv");
        let m = missing_art(&t.0.join("Bare (2021)"), false);
        assert!(m.poster && m.fanart && m.seasons.is_empty());
    }

    #[test]
    fn ids_come_from_the_folder_name_first() {
        let t = Tree::new();
        t.file("Show (2019) [tvdbid-358700]/Season 01/e.mkv");
        let (tmdb, tvdb, _) = known_ids(&t.0.join("Show (2019) [tvdbid-358700]"), true);
        assert_eq!(tmdb, None);
        assert_eq!(tvdb.as_deref(), Some("358700"));
        t.file("Film (2014) [tmdbid-137106]/f.mkv");
        let (tmdb, _, _) = known_ids(&t.0.join("Film (2014) [tmdbid-137106]"), false);
        assert_eq!(tmdb, Some(137106));
    }
}
