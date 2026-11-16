//! `rustyfin lyrics`: fetch lyrics for tracks that have none from LRCLIB
//! (roadmap 11k, owner 2026-11-16) and write them beside the track, as
//! `<track>.lrc` (synced) or `<track>.txt` (plain). Existing lyrics (a sidecar
//! or tags) are never touched, and no file is ever overwritten.
//!
//! A match is the same recording: LRCLIB's `/api/get` is asked by artist,
//! title, album and duration. Albums where LRCLIB had nothing for some tracks
//! (or only instrumentals) record `[lyrics] checked` in the album folder's
//! `.rustyfin.toml`, and are not asked again for 30 days (`--force` asks anyway).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustyfin_common::lyrics::sidecar_path;
use rustyfin_common::music_tags::{self, TrackQuery};
use rustyfin_common::naming::is_audio_file;
use rustyfin_common::rate_limit::RateLimiter;
use rustyfin_common::title_settings;
use serde::Deserialize;

use crate::scan::ScanOpts;

/// An album where LRCLIB lacked lyrics is asked again after this long.
const RECHECK_AFTER: Duration = Duration::from_secs(30 * 24 * 3600);

/// What LRCLIB knows about one recording.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Synced(String),
    Plain(String),
    Instrumental,
    NotFound,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

/// LRCLIB's answer from an `/api/get` response.
pub fn answer_from(status: u16, body: &str) -> anyhow::Result<Answer> {
    if status == 404 {
        return Ok(Answer::NotFound);
    }
    if !(200..300).contains(&status) {
        anyhow::bail!("LRCLIB answered {status}");
    }
    let r: Record = serde_json::from_str(body)?;
    let text = |s: Option<String>| s.filter(|t| !t.trim().is_empty());
    Ok(if let Some(s) = text(r.synced_lyrics) {
        Answer::Synced(s)
    } else if let Some(p) = text(r.plain_lyrics) {
        Answer::Plain(p)
    } else if r.instrumental {
        Answer::Instrumental
    } else {
        Answer::NotFound
    })
}

/// The file to write for an answer: the track's name with `.lrc` or `.txt`.
pub fn target(track: &Path, answer: &Answer) -> Option<(PathBuf, String)> {
    match answer {
        Answer::Synced(s) => Some((track.with_extension("lrc"), s.clone())),
        Answer::Plain(p) => Some((track.with_extension("txt"), p.clone())),
        _ => None,
    }
}

struct Lrclib {
    http: reqwest::Client,
    base: String,
    limiter: RateLimiter,
}

impl Lrclib {
    async fn get(&self, q: &TrackQuery) -> anyhow::Result<Answer> {
        self.limiter.wait().await;
        let resp = self
            .http
            .get(format!("{}/api/get", self.base))
            .query(&[
                ("artist_name", q.artist.as_str()),
                ("track_name", q.title.as_str()),
                ("album_name", q.album.as_str()),
                ("duration", &q.duration_secs.to_string()),
            ])
            .send()
            .await?;
        let status = resp.status().as_u16();
        let body = resp.text().await?;
        answer_from(status, &body)
    }
}

#[derive(Debug, Default)]
pub struct LyricsStats {
    pub tracks: usize,
    pub had: usize,
    pub synced: usize,
    pub plain: usize,
    pub instrumental: usize,
    pub missing: usize,
    pub recently_checked: usize,
    pub unreadable: usize,
    pub errors: usize,
}

/// Write `text` to `path` unless a file is already there.
fn write_new(path: &Path, text: &str) -> std::io::Result<bool> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            f.write_all(text.as_bytes())?;
            if !text.ends_with('\n') {
                f.write_all(b"\n")?;
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

/// Fetch missing lyrics for every track under `root`.
pub async fn fill_missing(root: &Path, opts: &ScanOpts<'_>) -> anyhow::Result<LyricsStats> {
    let cfg = &opts.config.lrclib;
    let client = Lrclib {
        http: reqwest::Client::builder()
            .user_agent(concat!(
                "Rustyfin/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/jkellz-dev/rustyfin)"
            ))
            .timeout(Duration::from_secs(30))
            .build()?,
        base: cfg.url.trim_end_matches('/').to_string(),
        limiter: RateLimiter::new(cfg.rate_limit, cfg.rate_window_secs),
    };

    let mut albums: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .flatten()
    {
        let p = entry.path();
        if p.is_file() && is_audio_file(p) {
            let dir = p.parent().unwrap_or(root).to_path_buf();
            albums.entry(dir).or_default().push(p.to_path_buf());
        }
    }

    let mut stats = LyricsStats::default();
    for (dir, mut tracks) in albums {
        tracks.sort();
        stats.tracks += tracks.len();
        let wanted: Vec<(PathBuf, TrackQuery)> = tracks
            .into_iter()
            .filter_map(|t| {
                if sidecar_path(&t).is_some() {
                    stats.had += 1;
                    return None;
                }
                match music_tags::read_track_query(&t) {
                    Some(q) if q.has_lyrics => {
                        stats.had += 1;
                        None
                    }
                    Some(q) => Some((t, q)),
                    None => {
                        stats.unreadable += 1;
                        None
                    }
                }
            })
            .collect();
        if wanted.is_empty() {
            continue;
        }
        if !opts.force
            && title_settings::load(&dir).is_some_and(|s| s.lyrics_checked_recently(RECHECK_AFTER))
        {
            stats.recently_checked += wanted.len();
            continue;
        }
        let mut lacking = false;
        let mut failed = false;
        for (track, q) in wanted {
            match client.get(&q).await {
                Ok(answer) => match target(&track, &answer) {
                    Some((path, text)) => {
                        if opts.dry_run {
                            tracing::info!("[dry run] would write {}", path.display());
                        } else if let Err(e) = write_new(&path, &text) {
                            tracing::warn!("cannot write {}: {e}", path.display());
                            stats.errors += 1;
                            failed = true;
                            continue;
                        }
                        match answer {
                            Answer::Synced(_) => stats.synced += 1,
                            _ => stats.plain += 1,
                        }
                    }
                    None => {
                        lacking = true;
                        if answer == Answer::Instrumental {
                            stats.instrumental += 1;
                        } else {
                            stats.missing += 1;
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!("LRCLIB failed for {}: {e}", track.display());
                    stats.errors += 1;
                    failed = true;
                }
            }
        }
        if lacking
            && !failed
            && !opts.dry_run
            && let Err(e) = title_settings::record_lyrics_checked_now(&dir)
        {
            tracing::warn!("cannot record the lyrics check in {}: {e}", dir.display());
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_lyrics_win_over_plain() {
        let body =
            r#"{"id":1,"instrumental":false,"plainLyrics":"a","syncedLyrics":"[00:01.00]a"}"#;
        assert_eq!(
            answer_from(200, body).unwrap(),
            Answer::Synced("[00:01.00]a".into())
        );
    }

    #[test]
    fn plain_when_not_synced() {
        let body = r#"{"instrumental":false,"plainLyrics":"words","syncedLyrics":null}"#;
        assert_eq!(
            answer_from(200, body).unwrap(),
            Answer::Plain("words".into())
        );
    }

    #[test]
    fn instrumentals_and_unknown_tracks_write_nothing() {
        let body = r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#;
        assert_eq!(answer_from(200, body).unwrap(), Answer::Instrumental);
        assert_eq!(
            answer_from(404, r#"{"code":404}"#).unwrap(),
            Answer::NotFound
        );
        assert!(target(Path::new("a.flac"), &Answer::Instrumental).is_none());
        assert!(answer_from(500, "").is_err());
    }

    #[test]
    fn targets_sit_beside_the_track() {
        let (p, _) = target(Path::new("/m/01 Song.flac"), &Answer::Synced("x".into())).unwrap();
        assert_eq!(p, Path::new("/m/01 Song.lrc"));
        let (p, _) = target(Path::new("/m/01 Song.flac"), &Answer::Plain("x".into())).unwrap();
        assert_eq!(p, Path::new("/m/01 Song.txt"));
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("rustyfin-lrclib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.lrc");
        assert!(write_new(&p, "first").unwrap());
        assert!(!write_new(&p, "second").unwrap());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "first\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
