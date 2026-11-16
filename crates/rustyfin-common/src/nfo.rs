use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Shared metadata struct returned by both NFO parsing and TMDB lookups.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NfoMetadata {
    pub title: Option<String>,
    pub sort_name: Option<String>,
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
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub album_type: Option<String>,
    pub artist_name: Option<String>,
    /// Transient: artwork URL for poster (not serialized to NFO XML).
    pub poster_url: Option<String>,
    /// Transient: artwork URL for backdrop (not serialized to NFO XML).
    pub backdrop_url: Option<String>,
    pub people: Vec<NfoPerson>,
    pub formed: Option<String>,
    pub disbanded: Option<String>,
    pub artist_type: Option<String>,
    pub country: Option<String>,
    pub disambiguation: Option<String>,
    pub tracks: Vec<NfoTrack>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NfoPerson {
    pub name: String,
    pub person_type: String,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}

/// Track entry within an album NFO (Rustyfin extension — ignored by Kodi).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NfoTrack {
    pub title: String,
    pub position: Option<i32>,
    pub disc: Option<i32>,
    pub duration: Option<f64>,
    pub file: Option<String>,
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
    tmdbid: Option<String>,
    imdbid: Option<String>,
    imdb_id: Option<String>,
    tvdbid: Option<String>,
    #[serde(default)]
    actor: Vec<NfoActor>,
    #[serde(default)]
    director: Vec<String>,
    #[serde(default)]
    credits: Vec<String>,
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
    tmdbid: Option<String>,
    imdbid: Option<String>,
    imdb_id: Option<String>,
    tvdbid: Option<String>,
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
    #[serde(default)]
    director: Vec<String>,
    #[serde(default)]
    credits: Vec<String>,
}

/// Root element for artist NFO files (Kodi `<artist>` format).
#[derive(Debug, Deserialize)]
struct ArtistNfo {
    name: Option<String>,
    sortname: Option<String>,
    #[serde(rename = "musicbrainzartistid")]
    musicbrainz_artist_id: Option<String>,
    biography: Option<String>,
    #[serde(default)]
    genre: Vec<String>,
    born: Option<String>,
    formed: Option<String>,
    died: Option<String>,
    disbanded: Option<String>,
}

/// Root element for album NFO files (Kodi `<album>` format).
#[derive(Debug, Deserialize)]
struct AlbumNfo {
    title: Option<String>,
    #[serde(rename = "artistdesc")]
    artist_name: Option<String>,
    #[serde(rename = "musicbrainzalbumid")]
    musicbrainz_album_id: Option<String>,
    #[serde(rename = "musicbrainzreleasegroupid")]
    musicbrainz_release_group_id: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    genre: Vec<String>,
    label: Option<String>,
    review: Option<String>,
    ratings: Option<NfoRatings>,
    #[serde(rename = "type")]
    album_type: Option<String>,
    #[serde(default)]
    track: Vec<AlbumNfoTrack>,
}

/// A track duration in seconds: `259.0` (Kodi), or `mm:ss` / `h:mm:ss`
/// (Lidarr). Anything else is no duration, not a broken album NFO.
fn de_track_duration<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw.as_deref().and_then(parse_track_duration))
}

fn parse_track_duration(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if let Ok(secs) = raw.parse::<f64>() {
        return (secs.is_finite() && secs >= 0.0).then_some(secs);
    }
    let parts: Vec<&str> = raw.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut total = 0.0;
    for part in parts {
        let n: f64 = part.trim().parse().ok()?;
        if !n.is_finite() || n < 0.0 {
            return None;
        }
        total = total * 60.0 + n;
    }
    Some(total)
}

#[derive(Debug, Deserialize)]
struct AlbumNfoTrack {
    title: Option<String>,
    position: Option<i32>,
    disc: Option<i32>,
    #[serde(default, deserialize_with = "de_track_duration")]
    duration: Option<f64>,
    file: Option<String>,
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
    /// Jellyfin's name for `order`.
    sortorder: Option<i32>,
    thumb: Option<String>,
    tmdbid: Option<String>,
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

/// Find an artist NFO file: `artist.nfo` in the artist directory.
pub fn find_artist_nfo(artist_dir: &Path) -> Option<PathBuf> {
    let nfo = artist_dir.join("artist.nfo");
    if nfo.is_file() { Some(nfo) } else { None }
}

/// Find an album NFO file: `album.nfo` in the album directory.
pub fn find_album_nfo(album_dir: &Path) -> Option<PathBuf> {
    let nfo = album_dir.join("album.nfo");
    if nfo.is_file() { Some(nfo) } else { None }
}

// ── Parsing functions ───────────────────────────────────────────

/// Parse a movie NFO file into `NfoMetadata`.
pub fn parse_movie_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    match quick_xml::de::from_str::<MovieNfo>(&content) {
        Ok(nfo) => Some(movie_nfo_to_metadata(nfo)),
        Err(e) => {
            tracing::warn!("Failed to parse movie NFO {}: {e}", path.display());
            None
        }
    }
}

/// Only the film series of a movie NFO.
#[derive(Debug, Deserialize)]
struct MovieSetOnly {
    set: Option<NfoSet>,
}

/// `<set><name>Alien Collection</name><overview>…</overview></set>` (Radarr,
/// Kodi 17+) or `<set>Alien Collection</set>` (older tools).
#[derive(Debug, Deserialize)]
struct NfoSet {
    #[serde(rename = "$text")]
    text: Option<String>,
    name: Option<String>,
    /// Radarr: `<set tmdbcolid="1267164">`.
    #[serde(rename = "@tmdbcolid")]
    tmdbcolid: Option<String>,
}

/// The film series (collection) a movie NFO puts the film in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieSet {
    pub name: String,
    /// The collection's TMDB id, when the NFO gives it (Radarr does).
    pub tmdb_id: Option<String>,
}

/// The film series a movie NFO names, if any, with its TMDB id if given.
pub fn parse_movie_set(path: &Path) -> Option<MovieSet> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    let set = quick_xml::de::from_str::<MovieSetOnly>(&content)
        .ok()?
        .set?;
    let name = set
        .name
        .or(set.text)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    let tmdb_id = set
        .tmdbcolid
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    Some(MovieSet { name, tmdb_id })
}

/// The film series (collection) a movie NFO names, if any.
pub fn parse_movie_collection(path: &Path) -> Option<String> {
    parse_movie_set(path).map(|s| s.name)
}

/// Parse a tvshow NFO file into `NfoMetadata`.
pub fn parse_series_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    match quick_xml::de::from_str::<TvShowNfo>(&content) {
        Ok(nfo) => Some(tvshow_nfo_to_metadata(nfo)),
        Err(e) => {
            tracing::warn!("Failed to parse series NFO {}: {e}", path.display());
            None
        }
    }
}

/// Parse an episode NFO file into `NfoMetadata`.
pub fn parse_episode_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    match quick_xml::de::from_str::<EpisodeNfo>(&content) {
        Ok(nfo) => Some(episode_nfo_to_metadata(nfo)),
        Err(e) => {
            tracing::warn!("Failed to parse episode NFO {}: {e}", path.display());
            None
        }
    }
}

/// Parse an artist NFO file into `NfoMetadata`.
pub fn parse_artist_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    match quick_xml::de::from_str::<ArtistNfo>(&content) {
        Ok(nfo) => Some(artist_nfo_to_metadata(nfo)),
        Err(e) => {
            tracing::warn!("Failed to parse artist NFO {}: {e}", path.display());
            None
        }
    }
}

/// Parse an album NFO file into `NfoMetadata`.
pub fn parse_album_nfo(path: &Path) -> Option<NfoMetadata> {
    let content = read_nfo(path)?;
    if is_url_only(&content) {
        return None;
    }
    match quick_xml::de::from_str::<AlbumNfo>(&content) {
        Ok(nfo) => Some(album_nfo_to_metadata(nfo)),
        Err(e) => {
            tracing::warn!("Failed to parse album NFO {}: {e}", path.display());
            None
        }
    }
}

fn read_nfo(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        // Jellyfin starts its NFOs with a UTF-8 byte-order mark.
        Ok(content) => Some(match content.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_string(),
            None => content,
        }),
        Err(e) => {
            tracing::warn!("Failed to read NFO {}: {e}", path.display());
            None
        }
    }
}

// ── Conversion helpers ──────────────────────────────────────────

fn movie_nfo_to_metadata(nfo: MovieNfo) -> NfoMetadata {
    let mut meta = NfoMetadata {
        title: nfo.title,
        original_title: nfo.originaltitle,
        overview: nfo.plot,
        genres: nfo.genre.into_iter().filter(|s| !s.is_empty()).collect(),
        studios: nfo.studio.into_iter().filter(|s| !s.is_empty()).collect(),
        official_rating: nfo.mpaa,
        production_year: nfo.year,
        premiere_date: nfo.premiered.as_deref().and_then(parse_date),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    apply_unique_ids(&nfo.uniqueid, &mut meta);
    apply_plain_ids(
        [&nfo.tmdbid, &nfo.imdbid, &nfo.imdb_id, &nfo.tvdbid],
        &mut meta,
    );
    apply_actors(&nfo.actor, &mut meta);

    for director in &nfo.director {
        for name in director.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Director".to_string(),
                role: None,
                sort_order: None,
                image_url: None,
                provider_person_id: None,
            });
        }
    }

    for credits in &nfo.credits {
        for name in credits.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Writer".to_string(),
                role: None,
                sort_order: None,
                image_url: None,
                provider_person_id: None,
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
        genres: nfo.genre.into_iter().filter(|s| !s.is_empty()).collect(),
        studios: nfo.studio.into_iter().filter(|s| !s.is_empty()).collect(),
        official_rating: nfo.mpaa,
        production_year: nfo.year,
        premiere_date: nfo.premiered.as_deref().and_then(parse_date),
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
    apply_unique_ids(&nfo.uniqueid, &mut meta);
    apply_plain_ids(
        [&nfo.tmdbid, &nfo.imdbid, &nfo.imdb_id, &nfo.tvdbid],
        &mut meta,
    );
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

    for director in &nfo.director {
        for name in director.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Director".to_string(),
                role: None,
                sort_order: None,
                image_url: None,
                provider_person_id: None,
            });
        }
    }

    for credits in &nfo.credits {
        for name in credits.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            meta.people.push(NfoPerson {
                name: name.to_string(),
                person_type: "Writer".to_string(),
                role: None,
                sort_order: None,
                image_url: None,
                provider_person_id: None,
            });
        }
    }

    meta
}

fn artist_nfo_to_metadata(nfo: ArtistNfo) -> NfoMetadata {
    NfoMetadata {
        title: nfo.name,
        sort_name: nfo.sortname,
        overview: nfo.biography,
        genres: nfo.genre,
        musicbrainz_artist_id: nfo.musicbrainz_artist_id,
        premiere_date: nfo
            .born
            .as_deref()
            .or(nfo.formed.as_deref())
            .and_then(parse_date),
        end_date: nfo
            .died
            .as_deref()
            .or(nfo.disbanded.as_deref())
            .and_then(parse_date),
        ..Default::default()
    }
}

fn album_nfo_to_metadata(nfo: AlbumNfo) -> NfoMetadata {
    let tracks = nfo
        .track
        .into_iter()
        .map(|t| NfoTrack {
            title: t.title.unwrap_or_default(),
            position: t.position,
            disc: t.disc,
            duration: t.duration,
            file: t.file,
        })
        .collect();

    let mut meta = NfoMetadata {
        title: nfo.title,
        artist_name: nfo.artist_name,
        overview: nfo.review,
        genres: nfo.genre,
        production_year: nfo.year,
        studios: nfo.label.into_iter().collect(),
        musicbrainz_album_id: nfo.musicbrainz_album_id,
        musicbrainz_release_group_id: nfo.musicbrainz_release_group_id,
        album_type: nfo.album_type,
        tracks,
        ..Default::default()
    };

    apply_ratings(&nfo.ratings, &mut meta);
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
            "musicbrainzartist" => meta.musicbrainz_artist_id = Some(val.clone()),
            "musicbrainzalbum" => meta.musicbrainz_album_id = Some(val.clone()),
            "musicbrainzreleasegroup" => meta.musicbrainz_release_group_id = Some(val.clone()),
            _ => {}
        }
    }
}

/// Ids as plain top-level elements, as Jellyfin's NFO saver writes them
/// (`<tmdbid>802217</tmdbid>`, `<imdbid>`, `<imdb_id>`, `<tvdbid>`), used
/// where `<uniqueid>` did not give them.
fn apply_plain_ids(
    [tmdbid, imdbid, imdb_id, tvdbid]: [&Option<String>; 4],
    meta: &mut NfoMetadata,
) {
    let clean = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    if meta.tmdb_id.is_none() {
        meta.tmdb_id = clean(tmdbid);
    }
    if meta.imdb_id.is_none() {
        meta.imdb_id = clean(imdbid).or_else(|| clean(imdb_id));
    }
    if meta.tvdb_id.is_none() {
        meta.tvdb_id = clean(tvdbid);
    }
}

/// An actor photo another program can fetch: a web address. Jellyfin's NFO
/// saver writes paths inside its own config volume
/// (`/config/metadata/People/N/Name/folder.jpg`), which nothing else can read.
fn fetchable_photo(thumb: &Option<String>) -> Option<String> {
    thumb
        .as_deref()
        .map(str::trim)
        .filter(|t| t.starts_with("https://") || t.starts_with("http://"))
        .map(str::to_string)
}

fn apply_actors(actors: &[NfoActor], meta: &mut NfoMetadata) {
    for (i, actor) in actors.iter().enumerate() {
        let Some(ref name) = actor.name else { continue };
        if name.is_empty() {
            continue;
        }
        let provider_person_id = actor.tmdbid.as_ref().map(|id| format!("tmdb-{id}"));
        meta.people.push(NfoPerson {
            name: name.clone(),
            person_type: "Actor".to_string(),
            role: actor.role.clone(),
            sort_order: actor.order.or(actor.sortorder).or(Some(i as i32)),
            image_url: fetchable_photo(&actor.thumb),
            provider_person_id,
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
#[serde(rename = "artist")]
struct WriteArtistNfo {
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sortname: Option<String>,
    #[serde(
        rename = "musicbrainzartistid",
        skip_serializing_if = "Option::is_none"
    )]
    musicbrainz_artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    biography: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    formed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disbanded: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    artist_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disambiguation: Option<String>,
}

#[derive(Serialize)]
#[serde(rename = "album")]
struct WriteAlbumNfo {
    title: Option<String>,
    #[serde(rename = "artistdesc", skip_serializing_if = "Option::is_none")]
    artist_name: Option<String>,
    #[serde(rename = "musicbrainzalbumid", skip_serializing_if = "Option::is_none")]
    musicbrainz_album_id: Option<String>,
    #[serde(
        rename = "musicbrainzreleasegroupid",
        skip_serializing_if = "Option::is_none"
    )]
    musicbrainz_release_group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genre: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    album_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    track: Vec<WriteAlbumTrack>,
}

#[derive(Serialize)]
struct WriteAlbumTrack {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disc: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    thumb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tmdbid: Option<String>,
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
pub fn write_series_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
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

/// Write artist metadata to an NFO file.
pub fn write_artist_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let nfo = WriteArtistNfo {
        name: meta.title.clone(),
        sortname: meta.sort_name.clone(),
        musicbrainz_artist_id: meta.musicbrainz_artist_id.clone(),
        biography: meta.overview.clone(),
        genre: meta.genres.clone(),
        formed: meta.formed.clone(),
        disbanded: meta.disbanded.clone(),
        artist_type: meta.artist_type.clone(),
        country: meta.country.clone(),
        disambiguation: meta.disambiguation.clone(),
    };

    let xml = quick_xml::se::to_string(&nfo)?;
    let content = format!("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n{xml}");
    std::fs::write(path, content)?;
    Ok(())
}

/// Write album metadata to an NFO file.
pub fn write_album_nfo(path: &Path, meta: &NfoMetadata) -> Result<(), Box<dyn std::error::Error>> {
    let tracks = meta
        .tracks
        .iter()
        .map(|t| WriteAlbumTrack {
            title: t.title.clone(),
            position: t.position,
            disc: t.disc,
            duration: t.duration,
            file: t.file.clone(),
        })
        .collect();

    let nfo = WriteAlbumNfo {
        title: meta.title.clone(),
        artist_name: meta.artist_name.clone(),
        musicbrainz_album_id: meta.musicbrainz_album_id.clone(),
        musicbrainz_release_group_id: meta.musicbrainz_release_group_id.clone(),
        year: meta.production_year,
        genre: meta.genres.clone(),
        label: meta.studios.first().cloned(),
        review: meta.overview.clone(),
        album_type: meta.album_type.clone(),
        track: tracks,
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
    if let Some(ref mb) = meta.musicbrainz_artist_id {
        ids.push(WriteUniqueId {
            id_type: "musicbrainzartist".to_string(),
            value: mb.clone(),
        });
    }
    if let Some(ref mb) = meta.musicbrainz_album_id {
        ids.push(WriteUniqueId {
            id_type: "musicbrainzalbum".to_string(),
            value: mb.clone(),
        });
    }
    if let Some(ref mb) = meta.musicbrainz_release_group_id {
        ids.push(WriteUniqueId {
            id_type: "musicbrainzreleasegroup".to_string(),
            value: mb.clone(),
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
            thumb: p.image_url.clone(),
            tmdbid: p
                .provider_person_id
                .as_ref()
                .and_then(|pid| pid.strip_prefix("tmdb-").map(|s| s.to_string())),
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

    #[test]
    fn test_write_and_read_artist_nfo() {
        let meta = NfoMetadata {
            title: Some("Pink Floyd".to_string()),
            overview: Some("English rock band.".to_string()),
            genres: vec!["Rock".to_string(), "Progressive Rock".to_string()],
            musicbrainz_artist_id: Some("83d91898-7763-47d7-b03b-b92132375c47".to_string()),
            ..Default::default()
        };
        let dir = std::env::temp_dir().join("rustyfin-test-artist-nfo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("artist.nfo");
        write_artist_nfo(&path, &meta).unwrap();

        let found = find_artist_nfo(&dir);
        assert!(found.is_some());

        let read_back = parse_artist_nfo(&path).unwrap();
        assert_eq!(read_back.title, Some("Pink Floyd".to_string()));
        assert_eq!(read_back.overview, Some("English rock band.".to_string()));
        assert_eq!(read_back.genres, vec!["Rock", "Progressive Rock"]);
        assert_eq!(
            read_back.musicbrainz_artist_id,
            Some("83d91898-7763-47d7-b03b-b92132375c47".to_string())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_and_read_album_nfo() {
        let meta = NfoMetadata {
            title: Some("The Dark Side of the Moon".to_string()),
            overview: Some("Eighth studio album.".to_string()),
            genres: vec!["Progressive Rock".to_string()],
            production_year: Some(1973),
            studios: vec!["Harvest Records".to_string()],
            ..Default::default()
        };
        let dir = std::env::temp_dir().join("rustyfin-test-album-nfo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("album.nfo");
        write_album_nfo(&path, &meta).unwrap();

        let found = find_album_nfo(&dir);
        assert!(found.is_some());

        let read_back = parse_album_nfo(&path).unwrap();
        assert_eq!(
            read_back.title,
            Some("The Dark Side of the Moon".to_string())
        );
        assert_eq!(read_back.overview, Some("Eighth studio album.".to_string()));
        assert_eq!(read_back.genres, vec!["Progressive Rock"]);
        assert_eq!(read_back.production_year, Some(1973));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parse_movie_nfo_with_duplicate_directors() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<movie>
  <title>The Many Adventures of Winnie the Pooh</title>
  <plot>A lovely movie.</plot>
  <director>John Lounsbery</director>
  <director>Wolfgang Reitherman</director>
  <year>1977</year>
  <genre>Animation</genre>
  <genre>Family</genre>
  <studio />
  <studio>Walt Disney Productions</studio>
  <trailer>plugin://something</trailer>
  <country />
  <country>United States</country>
  <tag>cartoon</tag>
  <sorttitle>many adventures of winnie the pooh</sorttitle>
</movie>"#;
        let result: Result<MovieNfo, _> = quick_xml::de::from_str(xml);
        assert!(result.is_ok(), "Failed to parse: {result:?}");
        let nfo = result.unwrap();
        assert_eq!(
            nfo.title,
            Some("The Many Adventures of Winnie the Pooh".to_string())
        );
    }

    // ── Helper for temp NFO files ───────────────────────────────

    fn write_temp_nfo(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustyfin-test-nfo-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.nfo"));
        std::fs::write(&path, content).unwrap();
        path
    }

    fn cleanup_temp_nfo(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent).ok();
        }
    }

    // ── Test 1: Movie NFO — Justice League (comprehensive) ──────

    #[test]
    fn nfo_movie_justice_league_comprehensive() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<movie>
    <title>Justice League</title>
    <originaltitle>Justice League</originaltitle>
    <ratings>
        <rating name="imdb" max="10" default="true">
            <value>6.400000</value>
            <votes>335583</votes>
        </rating>
        <rating name="themoviedb" max="10">
            <value>6.200000</value>
        </rating>
    </ratings>
    <plot>Fueled by his restored faith in humanity.</plot>
    <mpaa>Australia:M</mpaa>
    <uniqueid type="imdb">tt0974015</uniqueid>
    <uniqueid type="tmdb">141052</uniqueid>
    <genre>Action</genre>
    <genre>Adventure</genre>
    <genre>Fantasy</genre>
    <genre>Sci-Fi</genre>
    <premiered>2017-11-15</premiered>
    <year>2017</year>
    <studio>DC Comics</studio>
    <director>Zack Snyder</director>
    <credits>Jerry Siegel</credits>
    <credits>Joe Shuster</credits>
    <actor>
        <name>Ben Affleck</name>
        <role>Batman</role>
        <order>0</order>
        <thumb>https://example.com/affleck.jpg</thumb>
    </actor>
    <actor>
        <name>Jason Momoa</name>
        <role>Aquaman</role>
        <order>5</order>
        <thumb>https://example.com/momoa.jpg</thumb>
    </actor>
</movie>"#;

        let path = write_temp_nfo("justice-league", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        // Title and original title
        assert_eq!(meta.title.as_deref(), Some("Justice League"));
        assert_eq!(meta.original_title.as_deref(), Some("Justice League"));

        // Genres
        assert_eq!(
            meta.genres,
            vec!["Action", "Adventure", "Fantasy", "Sci-Fi"]
        );

        // Studios
        assert_eq!(meta.studios, vec!["DC Comics"]);

        // Production year
        assert_eq!(meta.production_year, Some(2017));

        // Premiere date (2017-11-15)
        let premiere = meta.premiere_date.expect("premiere_date should be set");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "2017-11-15");

        // Provider IDs
        assert_eq!(meta.imdb_id.as_deref(), Some("tt0974015"));
        assert_eq!(meta.tmdb_id.as_deref(), Some("141052"));

        // Community rating: apply_ratings takes the first imdb/themoviedb match (IMDB = 6.4)
        let rating = meta
            .community_rating
            .expect("community_rating should be set");
        assert!(
            (rating - 6.4).abs() < 0.01,
            "Expected community_rating ~6.4, got {rating}"
        );

        // TODO: critic_rating is not parsed because MovieNfo struct lacks a `criticrating` field.
        // Jellyfin parses <criticrating> but rustyfin currently ignores it.
        assert!(
            meta.critic_rating.is_none(),
            "critic_rating is not currently parsed from movie NFO"
        );

        // Official rating (mpaa)
        assert_eq!(meta.official_rating.as_deref(), Some("Australia:M"));

        // Overview
        assert_eq!(
            meta.overview.as_deref(),
            Some("Fueled by his restored faith in humanity.")
        );

        // People: 2 actors + 1 director + 2 writers = 5
        assert_eq!(
            meta.people.len(),
            5,
            "Expected 5 people (2 actors + 1 director + 2 writers), got {}",
            meta.people.len()
        );

        // Check actor Jason Momoa
        let momoa = meta
            .people
            .iter()
            .find(|p| p.name == "Jason Momoa")
            .expect("Jason Momoa should be in people");
        assert_eq!(momoa.person_type, "Actor");
        assert_eq!(momoa.role.as_deref(), Some("Aquaman"));
        assert_eq!(momoa.sort_order, Some(5));
        assert!(
            momoa
                .image_url
                .as_ref()
                .is_some_and(|u| u.starts_with("https://")),
            "Jason Momoa image_url should start with https://"
        );

        // Check director Zack Snyder
        let snyder = meta
            .people
            .iter()
            .find(|p| p.name == "Zack Snyder")
            .expect("Zack Snyder should be in people");
        assert_eq!(snyder.person_type, "Director");

        // Check writers
        let siegel = meta
            .people
            .iter()
            .find(|p| p.name == "Jerry Siegel")
            .expect("Jerry Siegel should be in people");
        assert_eq!(siegel.person_type, "Writer");

        let shuster = meta
            .people
            .iter()
            .find(|p| p.name == "Joe Shuster")
            .expect("Joe Shuster should be in people");
        assert_eq!(shuster.person_type, "Writer");
    }

    // ── Test 2: Series NFO — American Gods (comprehensive) ──────

    #[test]
    fn nfo_series_american_gods_comprehensive() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<tvshow>
    <title>American Gods</title>
    <originaltitle>American Gods</originaltitle>
    <plot>An ex-con becomes the traveling partner of a conman.</plot>
    <ratings>
        <rating name="themoviedb" max="10" default="true">
            <value>6.800000</value>
        </rating>
    </ratings>
    <uniqueid type="tmdb" default="true">46639</uniqueid>
    <uniqueid type="tvdb">253573</uniqueid>
    <genre>Drama</genre>
    <genre>Mystery</genre>
    <genre>Sci-Fi &amp; Fantasy</genre>
    <premiered>2017-04-30</premiered>
    <year>2017</year>
    <studio>Starz</studio>
    <actor>
        <name>Ricky Whittle</name>
        <role>Shadow Moon</role>
        <order>0</order>
        <thumb>http://image.tmdb.org/t/p/original/cjeDbVfBp6Qvb3C74Dfy7BKDTQN.jpg</thumb>
    </actor>
    <actor>
        <name>Pablo Schreiber</name>
        <role>Mad Sweeney</role>
        <order>3</order>
        <thumb>http://image.tmdb.org/t/p/original/uo8YljeePz3pbj7gvWXdB4gOOW4.jpg</thumb>
    </actor>
</tvshow>"#;

        let path = write_temp_nfo("american-gods", xml);
        let meta = parse_series_nfo(&path).expect("parse_series_nfo returned None");
        cleanup_temp_nfo(&path);

        // Title
        assert_eq!(meta.title.as_deref(), Some("American Gods"));
        assert_eq!(meta.original_title.as_deref(), Some("American Gods"));

        // Genres — XML entity &amp; should decode to &
        assert_eq!(meta.genres, vec!["Drama", "Mystery", "Sci-Fi & Fantasy"]);

        // Provider IDs
        assert_eq!(meta.tmdb_id.as_deref(), Some("46639"));
        assert_eq!(meta.tvdb_id.as_deref(), Some("253573"));

        // Studios
        assert_eq!(meta.studios, vec!["Starz"]);

        // Premiere date
        let premiere = meta.premiere_date.expect("premiere_date should be set");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "2017-04-30");

        // Production year
        assert_eq!(meta.production_year, Some(2017));

        // Community rating
        let rating = meta
            .community_rating
            .expect("community_rating should be set");
        assert!(
            (rating - 6.8).abs() < 0.01,
            "Expected community_rating ~6.8, got {rating}"
        );

        // Overview
        assert_eq!(
            meta.overview.as_deref(),
            Some("An ex-con becomes the traveling partner of a conman.")
        );

        // People: 2 actors
        assert_eq!(meta.people.len(), 2);

        // Check Pablo Schreiber
        let pablo = meta
            .people
            .iter()
            .find(|p| p.name == "Pablo Schreiber")
            .expect("Pablo Schreiber should be in people");
        assert_eq!(pablo.person_type, "Actor");
        assert_eq!(pablo.role.as_deref(), Some("Mad Sweeney"));
        assert_eq!(pablo.sort_order, Some(3));
    }

    // ── Test 3: Episode NFO — Stargate Atlantis ─────────────────

    #[test]
    fn nfo_episode_stargate_atlantis_rising() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<episodedetails>
    <title>Rising</title>
    <season>1</season>
    <episode>1</episode>
    <plot>With the discovery of an Ancient outpost in the Antarctic, Dr. Daniel Jackson discovers the location of the mythical city.</plot>
    <aired>2004-07-16</aired>
    <uniqueid type="tvdb">55254</uniqueid>
    <actor>
        <name>Joe Flanigan</name>
        <role>Maj. John Sheppard</role>
        <order>0</order>
    </actor>
    <director>Martin Wood</director>
</episodedetails>"#;

        let path = write_temp_nfo("stargate-rising", xml);
        let meta = parse_episode_nfo(&path).expect("parse_episode_nfo returned None");
        cleanup_temp_nfo(&path);

        // Title
        assert_eq!(meta.title.as_deref(), Some("Rising"));

        // Overview contains key text
        let overview = meta.overview.as_deref().expect("overview should be set");
        assert!(
            overview.contains("Ancient outpost"),
            "Overview should contain 'Ancient outpost', got: {overview}"
        );

        // Premiere date (aired)
        let premiere = meta.premiere_date.expect("premiere_date should be set");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "2004-07-16");

        // Provider IDs
        assert_eq!(meta.tvdb_id.as_deref(), Some("55254"));

        // People: 1 actor + 1 director = 2
        assert_eq!(
            meta.people.len(),
            2,
            "Expected 2 people (1 actor + 1 director), got {}",
            meta.people.len()
        );

        // Check actor
        let flanigan = meta
            .people
            .iter()
            .find(|p| p.name == "Joe Flanigan")
            .expect("Joe Flanigan should be in people");
        assert_eq!(flanigan.person_type, "Actor");
        assert_eq!(flanigan.role.as_deref(), Some("Maj. John Sheppard"));
        assert_eq!(flanigan.sort_order, Some(0));

        // Check director
        let wood = meta
            .people
            .iter()
            .find(|p| p.name == "Martin Wood")
            .expect("Martin Wood should be in people");
        assert_eq!(wood.person_type, "Director");
    }

    // ── Test 4: Movie NFO — Minimal (edge case) ────────────────

    #[test]
    fn nfo_movie_minimal() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Minimal Movie</title>
</movie>"#;

        let path = write_temp_nfo("minimal-movie", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Minimal Movie"));
        assert!(meta.original_title.is_none());
        assert!(meta.overview.is_none());
        assert!(meta.genres.is_empty());
        assert!(meta.studios.is_empty());
        assert!(meta.official_rating.is_none());
        assert!(meta.community_rating.is_none());
        assert!(meta.critic_rating.is_none());
        assert!(meta.production_year.is_none());
        assert!(meta.premiere_date.is_none());
        assert!(meta.end_date.is_none());
        assert!(meta.tmdb_id.is_none());
        assert!(meta.tvdb_id.is_none());
        assert!(meta.imdb_id.is_none());
        assert!(meta.people.is_empty());
    }

    // ── Test 5: Movie NFO — XML special characters ──────────────

    #[test]
    fn nfo_movie_xml_special_characters() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Lilo &amp; Stitch</title>
    <originaltitle>Lilo &amp; Stitch</originaltitle>
    <plot>&gt;&gt;A Hawaiian girl&lt;&lt;</plot>
    <genre>Animation</genre>
    <year>2002</year>
</movie>"#;

        let path = write_temp_nfo("lilo-stitch", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        // XML entities should be decoded
        assert_eq!(meta.title.as_deref(), Some("Lilo & Stitch"));
        assert_eq!(meta.original_title.as_deref(), Some("Lilo & Stitch"));

        let overview = meta.overview.as_deref().expect("overview should be set");
        assert!(
            overview.starts_with(">>"),
            "Overview should start with >>, got: {overview}"
        );
        assert!(
            overview.ends_with("<<"),
            "Overview should end with <<, got: {overview}"
        );

        assert_eq!(meta.genres, vec!["Animation"]);
        assert_eq!(meta.production_year, Some(2002));
    }

    // ── Test 6: Series NFO with IMDB ID in <id> tag ─────────────

    #[test]
    fn nfo_series_id_tag_attributes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tvshow>
    <title>Test Series</title>
    <id IMDB="tt11111" TMDB="46639">253573</id>
    <uniqueid type="tvdb">253573</uniqueid>
</tvshow>"#;

        let path = write_temp_nfo("test-series-id", xml);
        let meta = parse_series_nfo(&path).expect("parse_series_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Test Series"));

        // <uniqueid type="tvdb"> should be captured
        assert_eq!(meta.tvdb_id.as_deref(), Some("253573"));

        // TODO: Rustyfin does not parse <id IMDB="..." TMDB="..."> attributes.
        // Jellyfin's Kodi NFO parser does extract these. The IMDB/TMDB attributes
        // in the <id> tag are not captured by rustyfin's current parser.
        assert!(
            meta.imdb_id.is_none(),
            "IMDB ID from <id> attributes is not parsed (expected behavior)"
        );
    }

    // ── Test 7: Music Album NFO ─────────────────────────────────

    #[test]
    fn nfo_album_best_of_u2() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<album>
    <title>The Best of 1980-1990</title>
    <artistdesc>U2</artistdesc>
    <genre>Rock</genre>
    <genre>Pop</genre>
    <year>1998</year>
    <label>Island Records</label>
</album>"#;

        let path = write_temp_nfo("u2-best-of", xml);
        let meta = parse_album_nfo(&path).expect("parse_album_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("The Best of 1980-1990"));
        assert_eq!(meta.artist_name.as_deref(), Some("U2"));
        assert_eq!(meta.genres, vec!["Rock", "Pop"]);
        assert_eq!(meta.production_year, Some(1998));
        // Label is stored in studios
        assert_eq!(meta.studios, vec!["Island Records"]);
    }

    // ── Test 8: Music Artist NFO ────────────────────────────────

    #[test]
    fn nfo_artist_u2() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<artist>
    <name>U2</name>
    <musicbrainzartistid>a3cb23fc-acd3-4ce0-8f36-1e5aa6a18432</musicbrainzartistid>
    <biography>U2 are an Irish rock band.</biography>
    <genre>Rock</genre>
    <genre>Alternative Rock</genre>
</artist>"#;

        let path = write_temp_nfo("u2-artist", xml);
        let meta = parse_artist_nfo(&path).expect("parse_artist_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("U2"));
        assert_eq!(meta.overview.as_deref(), Some("U2 are an Irish rock band."));
        assert_eq!(meta.genres, vec!["Rock", "Alternative Rock"]);
        assert_eq!(
            meta.musicbrainz_artist_id.as_deref(),
            Some("a3cb23fc-acd3-4ce0-8f36-1e5aa6a18432")
        );
    }

    // ── Test 9: Multiple ratings — community_rating selection ───

    #[test]
    fn nfo_movie_multiple_ratings_picks_first() {
        // Rustyfin's apply_ratings takes the first imdb/themoviedb match it
        // encounters, regardless of the `default` attribute. With IMDB listed
        // first, community_rating should be 6.4 (the IMDB value).
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<movie>
    <title>Justice League</title>
    <ratings>
        <rating name="imdb" max="10" default="true">
            <value>6.400000</value>
            <votes>335583</votes>
        </rating>
        <rating name="themoviedb" max="10">
            <value>6.200000</value>
        </rating>
    </ratings>
</movie>"#;

        let path = write_temp_nfo("jl-ratings", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        let rating = meta
            .community_rating
            .expect("community_rating should be set");
        // TODO: Rustyfin picks the first matching rating by order, not by the
        // `default="true"` attribute. In this case the result is the same (IMDB
        // is both first and default), but if TMDB were listed first with
        // default="true" on IMDB, rustyfin would pick TMDB's value instead.
        // Jellyfin respects the `default` attribute.
        assert!(
            (rating - 6.4).abs() < 0.01,
            "Expected community_rating ~6.4 (first imdb rating), got {rating}"
        );

        // Verify the TMDB rating (6.2) was NOT selected
        assert!(
            (rating - 6.2).abs() > 0.1,
            "community_rating should NOT be 6.2 (the TMDB value)"
        );
    }

    // ── Test 10: Empty/malformed NFO — no crash ─────────────────

    #[test]
    fn nfo_movie_empty_no_crash() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie></movie>"#;

        let path = write_temp_nfo("empty-movie", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None for empty movie");
        cleanup_temp_nfo(&path);

        assert!(meta.title.is_none());
        assert!(meta.original_title.is_none());
        assert!(meta.overview.is_none());
        assert!(meta.genres.is_empty());
        assert!(meta.studios.is_empty());
        assert!(meta.official_rating.is_none());
        assert!(meta.community_rating.is_none());
        assert!(meta.critic_rating.is_none());
        assert!(meta.production_year.is_none());
        assert!(meta.premiere_date.is_none());
        assert!(meta.tmdb_id.is_none());
        assert!(meta.imdb_id.is_none());
        assert!(meta.tvdb_id.is_none());
        assert!(meta.people.is_empty());
    }

    // ── Additional edge-case tests ──────────────────────────────

    #[test]
    fn nfo_movie_url_only_returns_none() {
        // NFO files that are just URLs (e.g., IMDB links) should return None
        let content = "https://www.imdb.com/title/tt0974015/";
        let path = write_temp_nfo("url-only", content);
        let result = parse_movie_nfo(&path);
        cleanup_temp_nfo(&path);

        assert!(
            result.is_none(),
            "URL-only NFO should return None, got: {result:?}"
        );
    }

    #[test]
    fn nfo_episode_no_actors() {
        // Episode with no actors or directors — just metadata
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<episodedetails>
    <title>Pilot</title>
    <plot>The beginning.</plot>
    <aired>2020-01-01</aired>
</episodedetails>"#;

        let path = write_temp_nfo("pilot-no-actors", xml);
        let meta = parse_episode_nfo(&path).expect("parse_episode_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Pilot"));
        assert_eq!(meta.overview.as_deref(), Some("The beginning."));
        assert!(meta.people.is_empty());
        let premiere = meta.premiere_date.expect("premiere_date should be set");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "2020-01-01");
    }

    #[test]
    fn nfo_series_empty_no_crash() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tvshow></tvshow>"#;

        let path = write_temp_nfo("empty-series", xml);
        let meta =
            parse_series_nfo(&path).expect("parse_series_nfo returned None for empty tvshow");
        cleanup_temp_nfo(&path);

        assert!(meta.title.is_none());
        assert!(meta.genres.is_empty());
        assert!(meta.studios.is_empty());
        assert!(meta.people.is_empty());
    }

    #[test]
    fn nfo_episode_empty_no_crash() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<episodedetails></episodedetails>"#;

        let path = write_temp_nfo("empty-episode", xml);
        let meta = parse_episode_nfo(&path)
            .expect("parse_episode_nfo returned None for empty episodedetails");
        cleanup_temp_nfo(&path);

        assert!(meta.title.is_none());
        assert!(meta.overview.is_none());
        assert!(meta.premiere_date.is_none());
        assert!(meta.people.is_empty());
    }

    #[test]
    fn nfo_movie_actor_without_optional_fields() {
        // Actor with only a name — role, order, thumb should default gracefully
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Test</title>
    <actor>
        <name>Unknown Actor</name>
    </actor>
</movie>"#;

        let path = write_temp_nfo("actor-minimal", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.people.len(), 1);
        let actor = &meta.people[0];
        assert_eq!(actor.name, "Unknown Actor");
        assert_eq!(actor.person_type, "Actor");
        assert!(actor.role.is_none());
        // When order is not specified, apply_actors falls back to enumeration index
        assert_eq!(actor.sort_order, Some(0));
        assert!(actor.image_url.is_none());
    }

    #[test]
    fn nfo_movie_nonexistent_file_returns_none() {
        let path = std::env::temp_dir().join("rustyfin-test-nfo-nonexistent/does-not-exist.nfo");
        let result = parse_movie_nfo(&path);
        assert!(result.is_none(), "Nonexistent file should return None");
    }

    /// Lidarr writes a track's duration as `mm:ss` (or `h:mm:ss`). One odd
    /// value must not cost the whole album its NFO (owner's scan, 2026-11-16).
    #[test]
    fn nfo_album_track_durations_in_clock_form() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<album>
    <title>Live</title>
    <track><title>Short</title><position>1</position><duration>03:44</duration></track>
    <track><title>Long</title><position>2</position><duration>1:02:03</duration></track>
    <track><title>Seconds</title><position>3</position><duration>259.5</duration></track>
    <track><title>Odd</title><position>4</position><duration>n/a</duration></track>
</album>"#;
        let path = write_temp_nfo("clock-durations", xml);
        let meta = parse_album_nfo(&path).expect("the album still parses");
        cleanup_temp_nfo(&path);
        let d: Vec<Option<f64>> = meta.tracks.iter().map(|t| t.duration).collect();
        assert_eq!(d, [Some(224.0), Some(3723.0), Some(259.5), None]);
        assert_eq!(meta.tracks[3].title, "Odd");
    }

    #[test]
    fn nfo_album_with_tracks() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<album>
    <title>Abbey Road</title>
    <artistdesc>The Beatles</artistdesc>
    <year>1969</year>
    <genre>Rock</genre>
    <track>
        <title>Come Together</title>
        <position>1</position>
        <duration>259.0</duration>
    </track>
    <track>
        <title>Something</title>
        <position>2</position>
        <duration>182.0</duration>
    </track>
</album>"#;

        let path = write_temp_nfo("abbey-road", xml);
        let meta = parse_album_nfo(&path).expect("parse_album_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Abbey Road"));
        assert_eq!(meta.artist_name.as_deref(), Some("The Beatles"));
        assert_eq!(meta.production_year, Some(1969));
        assert_eq!(meta.tracks.len(), 2);
        assert_eq!(meta.tracks[0].title, "Come Together");
        assert_eq!(meta.tracks[0].position, Some(1));
        assert!((meta.tracks[0].duration.unwrap() - 259.0).abs() < 0.1);
        assert_eq!(meta.tracks[1].title, "Something");
        assert_eq!(meta.tracks[1].position, Some(2));
    }

    #[test]
    fn nfo_artist_with_formed_and_disbanded() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<artist>
    <name>The Beatles</name>
    <biography>The Beatles were an English rock band.</biography>
    <formed>1960-01-01</formed>
    <disbanded>1970-04-10</disbanded>
    <genre>Rock</genre>
</artist>"#;

        let path = write_temp_nfo("beatles-artist", xml);
        let meta = parse_artist_nfo(&path).expect("parse_artist_nfo returned None");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("The Beatles"));
        // `formed` is used as premiere_date via born/formed fallback
        let premiere = meta
            .premiere_date
            .expect("premiere_date should be set from formed");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "1960-01-01");
        // `disbanded` is used as end_date via died/disbanded fallback
        let end = meta
            .end_date
            .expect("end_date should be set from disbanded");
        assert_eq!(end.format("%Y-%m-%d").to_string(), "1970-04-10");
    }

    #[test]
    fn nfo_movie_slash_separated_directors() {
        // Some NFOs encode multiple directors in a single <director> tag with slashes
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Test Multi Director</title>
    <director>Anthony Russo / Joe Russo</director>
</movie>"#;

        let path = write_temp_nfo("multi-director", xml);
        let meta = parse_movie_nfo(&path).expect("parse_movie_nfo returned None");
        cleanup_temp_nfo(&path);

        let directors: Vec<&NfoPerson> = meta
            .people
            .iter()
            .filter(|p| p.person_type == "Director")
            .collect();
        assert_eq!(
            directors.len(),
            2,
            "Expected 2 directors from slash-separated value, got {}",
            directors.len()
        );
        assert!(directors.iter().any(|d| d.name == "Anthony Russo"));
        assert!(directors.iter().any(|d| d.name == "Joe Russo"));
    }

    // ── Edge-case tests (Phase 11b, Task 3) ─────────────────────

    #[test]
    fn nfo_empty_file_returns_none() {
        let path = write_temp_nfo("edge-empty", "");
        let result = parse_movie_nfo(&path);
        cleanup_temp_nfo(&path);
        assert!(result.is_none(), "Empty file should return None");
    }

    #[test]
    fn nfo_url_only_returns_none_https() {
        let path = write_temp_nfo("edge-url-https", "https://www.themoviedb.org/movie/12345");
        let result = parse_movie_nfo(&path);
        cleanup_temp_nfo(&path);
        assert!(
            result.is_none(),
            "HTTPS URL-only NFO should return None via is_url_only"
        );
    }

    #[test]
    fn nfo_http_url_only_returns_none() {
        let path = write_temp_nfo("edge-url-http", "http://www.imdb.com/title/tt1234567");
        let result = parse_movie_nfo(&path);
        cleanup_temp_nfo(&path);
        assert!(
            result.is_none(),
            "HTTP URL-only NFO should return None via is_url_only"
        );
    }

    #[test]
    fn nfo_malformed_xml_returns_none() {
        let path = write_temp_nfo("edge-malformed", "<movie><title>Broken<title></movie");
        let result = parse_movie_nfo(&path);
        cleanup_temp_nfo(&path);
        assert!(
            result.is_none(),
            "Malformed XML should return None without panicking"
        );
    }

    #[test]
    fn nfo_missing_title_returns_none_title() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <plot>A plot with no title element.</plot>
    <year>2020</year>
</movie>"#;
        let path = write_temp_nfo("edge-no-title", xml);
        let meta = parse_movie_nfo(&path).expect("parse should succeed even without title");
        cleanup_temp_nfo(&path);

        assert!(meta.title.is_none(), "title should be None when omitted");
        assert_eq!(
            meta.overview.as_deref(),
            Some("A plot with no title element.")
        );
        assert_eq!(meta.production_year, Some(2020));
    }

    #[test]
    fn nfo_unicode_in_text_fields() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>千と千尋の神隠し</title>
    <plot>少女千尋は不思議な町に迷い込む。</plot>
    <genre>アニメーション</genre>
    <studio>スタジオジブリ</studio>
</movie>"#;
        let path = write_temp_nfo("edge-unicode", xml);
        let meta = parse_movie_nfo(&path).expect("parse should handle Unicode");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("千と千尋の神隠し"));
        assert_eq!(
            meta.overview.as_deref(),
            Some("少女千尋は不思議な町に迷い込む。")
        );
        assert_eq!(meta.genres, vec!["アニメーション"]);
        assert_eq!(meta.studios, vec!["スタジオジブリ"]);
    }

    #[test]
    fn nfo_episode_missing_season_episode_numbers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<episodedetails>
    <title>Mystery Episode</title>
    <plot>No season or episode number provided.</plot>
</episodedetails>"#;
        let path = write_temp_nfo("edge-ep-no-numbers", xml);
        let meta =
            parse_episode_nfo(&path).expect("episode parse should succeed without season/episode");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Mystery Episode"));
        assert_eq!(
            meta.overview.as_deref(),
            Some("No season or episode number provided.")
        );
        // No season/episode fields are captured in NfoMetadata so there is nothing
        // extra to assert; the key requirement is that parsing does not fail.
    }

    #[test]
    fn nfo_duplicate_genres_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Duplicate Genre Movie</title>
    <genre>Action</genre>
    <genre>Drama</genre>
    <genre>Action</genre>
</movie>"#;
        let path = write_temp_nfo("edge-dup-genres", xml);
        let meta = parse_movie_nfo(&path).expect("parse should succeed");
        cleanup_temp_nfo(&path);

        // Documenting current behavior: duplicate genres are preserved (not deduplicated).
        assert_eq!(
            meta.genres,
            vec!["Action", "Drama", "Action"],
            "All 3 genre entries (including the duplicate) should be preserved"
        );
    }

    #[test]
    fn nfo_empty_genre_and_studio_filtered() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Filtered Movie</title>
    <genre></genre>
    <genre>Thriller</genre>
    <genre />
    <studio />
    <studio>Warner Bros.</studio>
    <studio></studio>
</movie>"#;
        let path = write_temp_nfo("edge-empty-genre-studio", xml);
        let meta = parse_movie_nfo(&path).expect("parse should succeed");
        cleanup_temp_nfo(&path);

        // movie_nfo_to_metadata filters empty strings from genres and studios
        assert_eq!(
            meta.genres,
            vec!["Thriller"],
            "Empty genre elements should be filtered out"
        );
        assert_eq!(
            meta.studios,
            vec!["Warner Bros."],
            "Empty studio elements should be filtered out"
        );
    }

    #[test]
    fn nfo_series_parse_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tvshow>
    <title>Breaking Bad</title>
    <plot>A high school chemistry teacher turned meth cook.</plot>
    <genre>Crime</genre>
    <genre>Drama</genre>
    <genre>Thriller</genre>
    <year>2008</year>
    <premiered>2008-01-20</premiered>
</tvshow>"#;
        let path = write_temp_nfo("edge-series-basic", xml);
        let meta = parse_series_nfo(&path).expect("parse_series_nfo should succeed");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Breaking Bad"));
        assert_eq!(
            meta.overview.as_deref(),
            Some("A high school chemistry teacher turned meth cook.")
        );
        assert_eq!(meta.genres, vec!["Crime", "Drama", "Thriller"]);
        assert_eq!(meta.production_year, Some(2008));
        let premiere = meta.premiere_date.expect("premiere_date should be set");
        assert_eq!(premiere.format("%Y-%m-%d").to_string(), "2008-01-20");
    }

    #[test]
    fn nfo_artist_parse_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<artist>
    <name>Pink Floyd</name>
    <biography>English rock band.</biography>
</artist>"#;
        let path = write_temp_nfo("edge-artist-basic", xml);
        let meta = parse_artist_nfo(&path).expect("parse_artist_nfo should succeed");
        cleanup_temp_nfo(&path);

        assert_eq!(meta.title.as_deref(), Some("Pink Floyd"));
        assert_eq!(meta.overview.as_deref(), Some("English rock band."));
    }
}
