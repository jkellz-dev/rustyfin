use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

static RE_SXXEXX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[Ss](\d{1,2})[Ee](\d{1,3})").unwrap());
static RE_XN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\d{1,2})x(\d{1,3})").unwrap());
static RE_YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b((?:19|20)\d{2})\b").unwrap());
static RE_QUALITY_TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(2160p|1080p|1080i|720p|480p|4k|uhd|hdr10?|dv|dolby\.?vision|remux|bluray|blu-ray|bdrip|brrip|webrip|web-dl|webdl|hdtv|dvdrip|x264|x265|h\.?264|h\.?265|hevc|avc|aac|dts|dts-hd|dts-x|truehd|atmos|flac|mp3|ac3|eac3|10bit|proper|repack|extended|unrated|directors\.?cut|theatrical)\b").unwrap()
});
static RE_BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[.*?\]").unwrap());
static RE_SEASON_FOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:Season\s*|Staffel\s*|Saison\s*|S)(\d{1,2})$").unwrap());
static RE_LEADING_ARTICLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(the|a|an|der|die|das|le|la|les|el|los|las)\s+").unwrap());
static RE_TRACK_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,3})(?:\s*[.\-]\s*|\s+)(.+)$").unwrap());

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "m2ts", "ts", "mov", "webm", "wmv", "flv", "mpg", "mpeg", "m4v", "3gp",
    "ogv", "divx", "vob", "iso",
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "m4a", "aac", "ogg", "opus", "wav", "wma", "ape", "alac", "aiff", "aif",
    "dsf", "dff", "wv", "mka",
];

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tbn"];

#[derive(Debug, Clone)]
pub struct ResolvedItem {
    pub item_type: String,
    pub title: String,
    pub sort_name: String,
    pub production_year: Option<i32>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub container: String,
}

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

pub fn resolve_movie(path: &Path) -> ResolvedItem {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");
    let container = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let cleaned = clean_title(stem);
    let year = extract_year(stem);

    // Remove trailing year from title if present
    let title = if let Some(y) = year {
        let year_str = format!("({y})");
        cleaned
            .replace(&year_str, "")
            .replace(&y.to_string(), "")
            .trim()
            .trim_end_matches(|c: char| c == '-' || c == '.' || c.is_whitespace())
            .to_string()
    } else {
        cleaned.clone()
    };

    let title = if title.is_empty() { cleaned } else { title };
    let sort_name = make_sort_name(&title);

    ResolvedItem {
        item_type: "Movie".to_string(),
        title,
        sort_name,
        production_year: year,
        index_number: None,
        parent_index_number: None,
        container,
    }
}

pub fn resolve_episode(path: &Path, _series_name: &str) -> Option<ResolvedItem> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let container = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (season, episode) = parse_episode_numbers(stem)?;

    // Try to extract episode title: text after the S##E## pattern
    let title = if let Some(caps) = RE_SXXEXX.find(stem) {
        let after = &stem[caps.end()..];
        let cleaned = clean_title(after);
        if cleaned.is_empty() {
            format!("Episode {episode}")
        } else {
            cleaned
        }
    } else if let Some(caps) = RE_XN.find(stem) {
        let after = &stem[caps.end()..];
        let cleaned = clean_title(after);
        if cleaned.is_empty() {
            format!("Episode {episode}")
        } else {
            cleaned
        }
    } else {
        format!("Episode {episode}")
    };

    let sort_name = make_sort_name(&title);

    Some(ResolvedItem {
        item_type: "Episode".to_string(),
        title,
        sort_name,
        production_year: None,
        index_number: Some(episode),
        parent_index_number: Some(season),
        container,
    })
}

pub fn resolve_audio_track(path: &Path) -> ResolvedItem {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");
    let container = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (track_num, title) = if let Some(caps) = RE_TRACK_NUMBER.captures(stem) {
        let num: i32 = caps[1].parse().unwrap_or(0);
        let title = caps[2].trim().to_string();
        (Some(num), title)
    } else {
        (None, stem.to_string())
    };

    let sort_name = make_sort_name(&title);

    ResolvedItem {
        item_type: "Audio".to_string(),
        title,
        sort_name,
        production_year: None,
        index_number: track_num,
        parent_index_number: None,
        container,
    }
}

/// Extract a year from an album directory name, e.g. "Album Name (2020)".
pub fn extract_album_year(name: &str) -> Option<i32> {
    extract_year(name)
}

fn parse_episode_numbers(text: &str) -> Option<(i32, i32)> {
    // Try S##E## first (highest priority)
    if let Some(caps) = RE_SXXEXX.captures(text) {
        let season: i32 = caps[1].parse().ok()?;
        let episode: i32 = caps[2].parse().ok()?;
        return Some((season, episode));
    }
    // Try #x## pattern
    if let Some(caps) = RE_XN.captures(text) {
        let season: i32 = caps[1].parse().ok()?;
        let episode: i32 = caps[2].parse().ok()?;
        return Some((season, episode));
    }
    None
}

pub fn parse_season_folder(name: &str) -> Option<i32> {
    let trimmed = name.trim();
    // Try named patterns first
    if let Some(caps) = RE_SEASON_FOLDER.captures(trimmed) {
        return caps[1].parse().ok();
    }
    // Try bare number (e.g., "01", "1")
    if let Ok(n) = trimmed.parse::<i32>()
        && (0..=99).contains(&n)
    {
        return Some(n);
    }
    // "Specials" = season 0
    if trimmed.eq_ignore_ascii_case("specials") || trimmed.eq_ignore_ascii_case("extras") {
        return Some(0);
    }
    None
}

pub fn make_sort_name(title: &str) -> String {
    let lower = title.to_lowercase();
    RE_LEADING_ARTICLE.replace(&lower, "").trim().to_string()
}

pub fn clean_title(raw: &str) -> String {
    // Remove brackets [...]
    let s = RE_BRACKETS.replace_all(raw, " ");
    // Remove quality/codec tags
    let s = RE_QUALITY_TAGS.replace_all(&s, " ");
    // Replace dots and underscores with spaces
    let s = s.replace(['.', '_'], " ");
    // Replace hyphens surrounded by spaces
    let s = s.replace(" - ", " ");
    // Collapse whitespace and trim
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_year(text: &str) -> Option<i32> {
    // Prefer year in parentheses
    if let Some(start) = text.find('(')
        && let Some(end) = text[start..].find(')')
    {
        let inner = &text[start + 1..start + end];
        if let Some(caps) = RE_YEAR.captures(inner) {
            return caps[1].parse().ok();
        }
    }
    // Fall back to last bare year
    RE_YEAR
        .captures_iter(text)
        .last()
        .and_then(|caps| caps[1].parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_file() {
        assert!(is_video_file(Path::new("movie.mkv")));
        assert!(is_video_file(Path::new("movie.MP4")));
        assert!(!is_video_file(Path::new("cover.jpg")));
        assert!(!is_video_file(Path::new("readme.txt")));
    }

    #[test]
    fn test_resolve_movie_with_year() {
        let item = resolve_movie(Path::new("/movies/Blade Runner (1982).mkv"));
        assert_eq!(item.title, "Blade Runner");
        assert_eq!(item.production_year, Some(1982));
        assert_eq!(item.container, "mkv");
        assert_eq!(item.item_type, "Movie");
    }

    #[test]
    fn test_resolve_movie_without_year() {
        let item = resolve_movie(Path::new("/movies/Casablanca.mp4"));
        assert_eq!(item.title, "Casablanca");
        assert_eq!(item.production_year, None);
        assert_eq!(item.container, "mp4");
    }

    #[test]
    fn test_resolve_movie_with_quality_tags() {
        let item = resolve_movie(Path::new("/movies/Inception.2010.1080p.BluRay.x264.mkv"));
        assert_eq!(item.title, "Inception");
        assert_eq!(item.production_year, Some(2010));
    }

    #[test]
    fn test_resolve_episode_sxxexx() {
        let item = resolve_episode(
            Path::new(
                "/tv/Breaking Bad/Season 1/Breaking.Bad.S01E03.And.the.Bag's.in.the.River.mkv",
            ),
            "Breaking Bad",
        )
        .unwrap();
        assert_eq!(item.item_type, "Episode");
        assert_eq!(item.index_number, Some(3));
        assert_eq!(item.parent_index_number, Some(1));
        assert_eq!(item.container, "mkv");
    }

    #[test]
    fn test_resolve_episode_xn() {
        let item = resolve_episode(Path::new("/tv/Show/1x05.Episode.Title.mkv"), "Show").unwrap();
        assert_eq!(item.index_number, Some(5));
        assert_eq!(item.parent_index_number, Some(1));
    }

    #[test]
    fn test_parse_season_folder() {
        assert_eq!(parse_season_folder("Season 1"), Some(1));
        assert_eq!(parse_season_folder("Season 01"), Some(1));
        assert_eq!(parse_season_folder("S01"), Some(1));
        assert_eq!(parse_season_folder("Staffel 3"), Some(3));
        assert_eq!(parse_season_folder("Saison 2"), Some(2));
        assert_eq!(parse_season_folder("01"), Some(1));
        assert_eq!(parse_season_folder("Specials"), Some(0));
        assert_eq!(parse_season_folder("Random Folder"), None);
    }

    #[test]
    fn test_make_sort_name() {
        assert_eq!(make_sort_name("The Matrix"), "matrix");
        assert_eq!(make_sort_name("A Beautiful Mind"), "beautiful mind");
        assert_eq!(make_sort_name("An Education"), "education");
        assert_eq!(make_sort_name("Der Untergang"), "untergang");
        assert_eq!(make_sort_name("Inception"), "inception");
    }

    #[test]
    fn test_clean_title() {
        assert_eq!(
            clean_title("Movie.Name.2020.1080p.BluRay.x264"),
            "Movie Name 2020"
        );
        assert_eq!(clean_title("[GROUP] Movie Name"), "Movie Name");
        assert_eq!(clean_title("Movie_Name"), "Movie Name");
    }

    #[test]
    fn test_is_audio_file() {
        assert!(is_audio_file(Path::new("track.flac")));
        assert!(is_audio_file(Path::new("song.MP3")));
        assert!(is_audio_file(Path::new("music.m4a")));
        assert!(is_audio_file(Path::new("audio.opus")));
        assert!(!is_audio_file(Path::new("video.mkv")));
        assert!(!is_audio_file(Path::new("cover.jpg")));
    }

    #[test]
    fn test_resolve_audio_track_with_number_dash() {
        let item = resolve_audio_track(Path::new("/music/Artist/Album/01 - Track Title.flac"));
        assert_eq!(item.item_type, "Audio");
        assert_eq!(item.title, "Track Title");
        assert_eq!(item.index_number, Some(1));
        assert_eq!(item.container, "flac");
    }

    #[test]
    fn test_resolve_audio_track_with_number_dot() {
        let item = resolve_audio_track(Path::new("/music/Artist/Album/03. Song Name.mp3"));
        assert_eq!(item.title, "Song Name");
        assert_eq!(item.index_number, Some(3));
    }

    #[test]
    fn test_resolve_audio_track_without_number() {
        let item = resolve_audio_track(Path::new("/music/Artist/Album/Song Title.flac"));
        assert_eq!(item.title, "Song Title");
        assert_eq!(item.index_number, None);
    }

    #[test]
    fn test_extract_album_year() {
        assert_eq!(extract_album_year("Album Name (2020)"), Some(2020));
        assert_eq!(extract_album_year("Album Name"), None);
        assert_eq!(extract_album_year("Greatest Hits (1998)"), Some(1998));
    }
}
