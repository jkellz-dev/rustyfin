use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Shared metadata struct returned by both NFO parsing and TMDB lookups.
#[derive(Debug, Default)]
pub struct NfoMetadata {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f32>,
    pub critic_rating: Option<f32>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub tmdb_id: Option<String>,
    pub tvdb_id: Option<String>,
    pub imdb_id: Option<String>,
    /// Transient: artwork URL for poster (not serialized to NFO XML).
    pub poster_url: Option<String>,
    /// Transient: artwork URL for backdrop (not serialized to NFO XML).
    pub backdrop_url: Option<String>,
    pub people: Vec<NfoPerson>,
}

#[derive(Debug)]
pub struct NfoPerson {
    pub name: String,
    pub person_type: String,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
}

// ── XML structures for deserialization ───────────────────────────

/// Root element for movie NFO files.
#[derive(Debug, Deserialize)]
struct MovieNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(default)]
    genre: Vec<String>,
    #[serde(default)]
    studio: Vec<String>,
    mpaa: Option<String>,
    ratings: Option<NfoRatings>,
    year: Option<i32>,
    premiered: Option<String>,
    #[serde(default)]
    uniqueid: Vec<NfoUniqueId>,
    #[serde(default)]
    actor: Vec<NfoActor>,
    director: Option<String>,
    credits: Option<String>,
}

/// Root element for tvshow NFO files.
#[derive(Debug, Deserialize)]
struct TvShowNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(default)]
    genre: Vec<String>,
    #[serde(default)]
    studio: Vec<String>,
    mpaa: Option<String>,
    ratings: Option<NfoRatings>,
    year: Option<i32>,
    premiered: Option<String>,
    #[serde(default)]
    uniqueid: Vec<NfoUniqueId>,
    #[serde(default)]
    actor: Vec<NfoActor>,
}

/// Root element for episodedetails NFO files.
#[derive(Debug, Deserialize)]
struct EpisodeNfo {
    title: Option<String>,
    plot: Option<String>,
    ratings: Option<NfoRatings>,
    aired: Option<String>,
    #[serde(default)]
    uniqueid: Vec<NfoUniqueId>,
    #[serde(default)]
    actor: Vec<NfoActor>,
    director: Option<String>,
    credits: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NfoRatings {
    #[serde(default)]
    rating: Vec<NfoRating>,
}

#[derive(Debug, Deserialize)]
struct NfoRating {
    #[serde(rename = "@name", default)]
    name: String,
    value: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct NfoUniqueId {
    #[serde(rename = "@type", default)]
    id_type: String,
    #[serde(rename = "$text", default)]
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NfoActor {
    name: Option<String>,
    role: Option<String>,
    order: Option<i32>,
}

// ── Discovery functions ─────────────────────────────────────────

/// Find a movie NFO file: `<stem>.nfo` or `movie.nfo` next to the video.
pub fn find_movie_nfo(video_path: &Path) -> Option<PathBuf> {
    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;

    let stem_nfo = parent.join(format!("{stem}.nfo"));
    if stem_nfo.is_file() {
        return Some(stem_nfo);
    }

    let movie_nfo = parent.join("movie.nfo");
    if movie_nfo.is_file() {
        return Some(movie_nfo);
    }

    None
}

/// Find a series NFO file: `tvshow.nfo` in the series directory.
pub fn find_series_nfo(series_dir: &Path) -> Option<PathBuf> {
    let nfo = series_dir.join("tvshow.nfo");
    if nfo.is_file() { Some(nfo) } else { None }
}

/// Find an episode NFO file: `<stem>.nfo` next to the video.
pub fn find_episode_nfo(video_path: &Path) -> Option<PathBuf> {
    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;

    let nfo = parent.join(format!("{stem}.nfo"));
    if nfo.is_file() { Some(nfo) } else { None }
}

// ── Parsing functions ───────────────────────────────────────────

/// Parse a movie NFO file into `NfoMetadata`.
pub fn parse_movie_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_url_only(&content) {
        return None;
    }
    let nfo: MovieNfo = quick_xml::de::from_str(&content).ok()?;
    Some(movie_nfo_to_metadata(nfo))
}

/// Parse a tvshow NFO file into `NfoMetadata`.
pub fn parse_series_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_url_only(&content) {
        return None;
    }
    let nfo: TvShowNfo = quick_xml::de::from_str(&content).ok()?;
    Some(tvshow_nfo_to_metadata(nfo))
}

/// Parse an episode NFO file into `NfoMetadata`.
pub fn parse_episode_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_url_only(&content) {
        return None;
    }
    let nfo: EpisodeNfo = quick_xml::de::from_str(&content).ok()?;
    Some(episode_nfo_to_metadata(nfo))
}

// ── Conversion helpers ──────────────────────────────────────────

fn movie_nfo_to_metadata(nfo: MovieNfo) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: nfo.title,
        original_title: nfo.originaltitle,
        overview: nfo.plot,
        genres: nfo.genre,
        studios: nfo.studio,
        official_rating: nfo.mpaa,
        production_year: nfo.year,
        premiere_date: nfo.premiered.as_deref().and_then(parse_date),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    apply_unique_ids(&nfo.uniqueid, &mut meta);
    apply_actors(&nfo.actor, &mut meta);

    if let Some(director) = nfo.director {
        for name in director.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Director".to_string(),
                role: None,
                sort_order: None,
            });
        }
    }

    if let Some(credits) = nfo.credits {
        for name in credits.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Writer".to_string(),
                role: None,
                sort_order: None,
            });
        }
    }

    meta
}

fn tvshow_nfo_to_metadata(nfo: TvShowNfo) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: nfo.title,
        original_title: nfo.originaltitle,
        overview: nfo.plot,
        genres: nfo.genre,
        studios: nfo.studio,
        official_rating: nfo.mpaa,
        production_year: nfo.year,
        premiere_date: nfo.premiered.as_deref().and_then(parse_date),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    apply_unique_ids(&nfo.uniqueid, &mut meta);
    apply_actors(&nfo.actor, &mut meta);

    meta
}

fn episode_nfo_to_metadata(nfo: EpisodeNfo) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: nfo.title,
        overview: nfo.plot,
        premiere_date: nfo.aired.as_deref().and_then(parse_date),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    apply_unique_ids(&nfo.uniqueid, &mut meta);
    apply_actors(&nfo.actor, &mut meta);

    if let Some(director) = nfo.director {
        for name in director.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Director".to_string(),
                role: None,
                sort_order: None,
            });
        }
    }

    if let Some(credits) = nfo.credits {
        for name in credits.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Writer".to_string(),
                role: None,
                sort_order: None,
            });
        }
    }

    meta
}

fn apply_ratings(ratings: &Option<NfoRatings>, meta: &mut NfoMetadata) {
    let Some(ratings) = ratings else { return };
    for r in &ratings.rating {
        let Some(val) = r.value else { continue };
        match r.name.as_str() {
            "imdb" | "themoviedb" | "" => {
                if meta.community_rating.is_none() {
                    meta.community_rating = Some(val);
                }
            }
            "metacritic" => {
                meta.critic_rating = Some(val);
            }
            _ => {}
        }
    }
}

fn apply_unique_ids(ids: &[NfoUniqueId], meta: &mut NfoMetadata) {
    for uid in ids {
        let Some(ref val) = uid.value else { continue };
        if val.is_empty() {
            continue;
        }
        match uid.id_type.as_str() {
            "tmdb" => meta.tmdb_id = Some(val.clone()),
            "tvdb" => meta.tvdb_id = Some(val.clone()),
            "imdb" => meta.imdb_id = Some(val.clone()),
            _ => {}
        }
    }
}

fn apply_actors(actors: &[NfoActor], meta: &mut NfoMetadata) {
    for (i, actor) in actors.iter().enumerate() {
        let Some(ref name) = actor.name else { continue };
        if name.is_empty() {
            continue;
        }
        meta.people.push(NfoPerson {
            name: name.clone(),
            person_type: "Actor".to_string(),
            role: actor.role.clone(),
            sort_order: actor.order.or(Some(i as i32)),
        });
    }
}

/// Detect NFO files that are just URLs (e.g., IMDB links) rather than XML.
fn is_url_only(content: &str) -> bool {
    let trimmed = content.trim();
    // If it starts with http and has no XML-like content, skip it
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

/// Parse a date string like "2023-01-15" into a DateTime<Utc>.
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc())
}

// ── XML structures for serialization (NFO writing) ──────────────

#[derive(Serialize)]
#[serde(rename = "movie")]
struct WriteMovieNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    studio: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mpaa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    premiered: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Serialize)]
#[serde(rename = "tvshow")]
struct WriteTvShowNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    studio: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mpaa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    premiered: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Serialize)]
#[serde(rename = "episodedetails")]
struct WriteEpisodeNfo {
    title: Option<String>,
    plot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aired: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uniqueid: Vec<WriteUniqueId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actor: Vec<WriteActor>,
}

#[derive(Serialize)]
struct WriteUniqueId {
    #[serde(rename = "@type")]
    id_type: String,
    #[serde(rename = "$text")]
    value: String,
}

#[derive(Serialize)]
struct WriteActor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<i32>,
}

// ── Write functions ─────────────────────────────────────────────

/// Write movie metadata to an NFO file.
pub fn write_movie_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteMovieNfo {
        title: meta.title.clone(),
        originaltitle: meta.original_title.clone(),
        plot: meta.overview.clone(),
        genre: meta.genres.clone(),
        studio: meta.studios.clone(),
        mpaa: meta.official_rating.clone(),
        year: meta.production_year,
        premiered: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}

/// Write TV series metadata to an NFO file.
pub fn write_series_nfo(
    path: &Path,
    meta: &NfoMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteTvShowNfo {
        title: meta.title.clone(),
        originaltitle: meta.original_title.clone(),
        plot: meta.overview.clone(),
        genre: meta.genres.clone(),
        studio: meta.studios.clone(),
        mpaa: meta.official_rating.clone(),
        year: meta.production_year,
        premiered: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}

/// Write episode metadata to an NFO file.
pub fn write_episode_nfo(
    path: &Path,
    meta: &NfoMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteEpisodeNfo {
        title: meta.title.clone(),
        plot: meta.overview.clone(),
        aired: meta.premiere_date.map(|d| d.format("%Y-%m-%d").to_string()),
        uniqueid: build_write_unique_ids(meta),
        actor: build_write_actors(&meta.people),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}

// ── Write helpers ───────────────────────────────────────────────

fn build_write_unique_ids(meta: &NfoMetadata) -> Vec<WriteUniqueId> {
    let mut ids = Vec::new();
    if let Some(ref tmdb) = meta.tmdb_id {
        ids.push(WriteUniqueId {
            id_type: "tmdb".to_string(),
            value: tmdb.clone(),
        });
    }
    if let Some(ref tvdb) = meta.tvdb_id {
        ids.push(WriteUniqueId {
            id_type: "tvdb".to_string(),
            value: tvdb.clone(),
        });
    }
    if let Some(ref imdb) = meta.imdb_id {
        ids.push(WriteUniqueId {
            id_type: "imdb".to_string(),
            value: imdb.clone(),
        });
    }
    ids
}

fn build_write_actors(people: &[NfoPerson]) -> Vec<WriteActor> {
    people
        .iter()
        .filter(|p| p.person_type == "Actor")
        .map(|p| WriteActor {
            name: p.name.clone(),
            role: p.role.clone(),
            order: p.sort_order,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_movie_nfo() {
        let meta = NfoMetadata {
            title: Some("Test Movie".to_string()),
            overview: Some("A test movie.".to_string()),
            genres: vec!["Action".to_string(), "Drama".to_string()],
            tmdb_id: Some("12345".to_string()),
            production_year: Some(2024),
            ..Default::default()
        };
        let dir = std::env::temp_dir().join("rustyfin-test-nfo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("movie.nfo");
        write_movie_nfo(&path, &meta).unwrap();
        let read_back = parse_movie_nfo(&path).unwrap();
        assert_eq!(read_back.title, Some("Test Movie".to_string()));
        assert_eq!(read_back.overview, Some("A test movie.".to_string()));
        assert_eq!(read_back.genres, vec!["Action", "Drama"]);
        assert_eq!(read_back.tmdb_id, Some("12345".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
