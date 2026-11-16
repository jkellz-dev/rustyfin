//! `.rustyfin.toml`: optional settings for one title, kept in the film's or
//! show's folder (owner, 2026-11-15). Most folders have none.
//!
//! ```toml
//! [ids]          # when the folder name or NFO picks the wrong match
//! tmdb = 802217
//!
//! [metadata]
//! title = "8-Bit Christmas"
//!
//! [artwork]
//! skip = false                              # never fetch artwork for this title
//! missing_checked = 2026-11-15T08:00:00Z    # written by `rustyfin artwork`
//!
//! [lyrics]                                  # in an album folder
//! checked = 2026-11-16T08:00:00Z            # written by `rustyfin lyrics`
//!
//! [collection]                              # written by `rustyfin artwork
//! tmdb = 1565                               #   --check-collections`
//! name = "28 Days/Weeks/Years Later Collection"
//! checked = 2026-11-16T08:00:00Z
//! ```
//!
//! Precedence: these settings, then the NFO, then the folder name, then
//! TMDB. The server only reads the file; `rustyfin artwork` writes
//! `missing_checked` and nothing else, keeping the rest of the file as it is.
//! A file that does not parse, or has a key this version does not know, is
//! ignored with a warning (and never rewritten).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};

/// The settings file's name in a title's folder.
pub const FILE_NAME: &str = ".rustyfin.toml";

/// Everything a `.rustyfin.toml` may say.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TitleSettings {
    pub ids: Ids,
    pub metadata: Metadata,
    pub artwork: Artwork,
    pub lyrics: Lyrics,
    pub collection: Collection,
}

/// Provider ids that replace the folder name's and the NFO's.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ids {
    pub tmdb: Option<i64>,
    pub tvdb: Option<i64>,
    pub imdb: Option<String>,
}

/// Metadata that replaces the NFO's and the folder name's.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metadata {
    pub title: Option<String>,
}

/// What `rustyfin artwork` does for this title.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Artwork {
    /// Never fetch artwork (posters, backdrops, headshots) for this title.
    pub skip: bool,
    /// When TMDB last had none of the missing poster or backdrop.
    #[serde(deserialize_with = "toml_time")]
    pub missing_checked: Option<DateTime<Utc>>,
}

/// `[lyrics]`, in an album folder.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lyrics {
    /// When LRCLIB last lacked lyrics for some of the album's tracks.
    #[serde(deserialize_with = "toml_time")]
    pub checked: Option<DateTime<Utc>>,
}

/// `[collection]`: the film series TMDB puts the film in, recorded by
/// `rustyfin artwork --check-collections` (owner, 2026-11-16: opt-in) when
/// the NFO gives no collection id. Wins over the NFO's `<set>`.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Collection {
    /// The TMDB collection id; none when TMDB has the film in no collection.
    pub tmdb: Option<i64>,
    /// The collection's name on TMDB.
    pub name: Option<String>,
    /// When TMDB was last asked.
    #[serde(deserialize_with = "toml_time")]
    pub checked: Option<DateTime<Utc>>,
}

/// A TOML offset date-time (`2026-11-15T08:00:00Z`) as UTC.
fn toml_time<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
    let raw = toml::value::Datetime::deserialize(d)?;
    DateTime::parse_from_rfc3339(&raw.to_string())
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|e| {
            serde::de::Error::custom(format!("{raw}: needs a date, time and offset ({e})"))
        })
}

impl TitleSettings {
    /// When TMDB last had no art for this title.
    pub fn missing_checked_at(&self) -> Option<DateTime<Utc>> {
        self.artwork.missing_checked
    }

    /// Whether TMDB was found to have no art within `within` before `now`.
    pub fn missing_checked_within(&self, now: DateTime<Utc>, within: std::time::Duration) -> bool {
        self.artwork.missing_checked.is_some_and(|at| {
            match now.signed_duration_since(at).to_std() {
                Ok(age) => age < within,
                // Recorded in the future (clock skew): still recent.
                Err(_) => true,
            }
        })
    }
}

impl TitleSettings {
    /// Whether LRCLIB was asked for this album's missing lyrics within `within`.
    pub fn lyrics_checked_recently(&self, within: std::time::Duration) -> bool {
        self.lyrics
            .checked
            .is_some_and(|at| match Utc::now().signed_duration_since(at).to_std() {
                Ok(age) => age < within,
                Err(_) => true,
            })
    }

    /// Whether TMDB was asked for this film's collection within `within`.
    pub fn collection_checked_recently(&self, within: std::time::Duration) -> bool {
        self.collection.checked.is_some_and(|at| {
            match Utc::now().signed_duration_since(at).to_std() {
                Ok(age) => age < within,
                Err(_) => true,
            }
        })
    }

    /// Whether TMDB was found to have no art within `within` of now.
    pub fn missing_checked_recently(&self, within: std::time::Duration) -> bool {
        self.missing_checked_within(Utc::now(), within)
    }
}

/// Record that TMDB has no art for the title in `dir`, as of now.
pub fn record_missing_checked_now(dir: &Path) -> std::io::Result<()> {
    record_missing_checked(dir, Utc::now())
}

/// The settings file of the title in `dir`.
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The settings file of the title in `dir`, if there is one.
pub fn existing(dir: &Path) -> Option<PathBuf> {
    let p = path(dir);
    p.is_file().then_some(p)
}

/// The settings of the title in `dir`: None when it has no file, or when the
/// file cannot be read or understood (logged).
pub fn load(dir: &Path) -> Option<TitleSettings> {
    let p = existing(dir)?;
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("cannot read {}: {e}; ignored", p.display());
            return None;
        }
    };
    match toml::from_str(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!("{} is not valid and is ignored: {e}", p.display());
            None
        }
    }
}

const HEADER: &str = "# Rustyfin settings for this title. Optional: delete the file to reset.\n\
                      # https://github.com/jkellz-dev/rustyfin/blob/main/docs/title-settings.md\n";

/// Record in the title's settings that TMDB had no art for it at `when`,
/// creating the file if needed. Only the `missing_checked` line is written:
/// comments, layout and every other key stay as they were. A file that does
/// not parse is left alone (error).
pub fn record_missing_checked(dir: &Path, when: DateTime<Utc>) -> std::io::Result<()> {
    let p = path(dir);
    let old = match std::fs::read_to_string(&p) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(text) = &old
        && let Err(e) = toml::from_str::<TitleSettings>(text)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is not valid, so it is left alone: {e}", p.display()),
        ));
    }
    let line = format!("missing_checked = {}", when.format("%Y-%m-%dT%H:%M:%SZ"));
    write_line(dir, old, "artwork", "missing_checked", &line)
}

/// Record that LRCLIB was asked for the missing lyrics of the album in `dir`,
/// now. Same rules as `record_missing_checked`.
pub fn record_lyrics_checked_now(dir: &Path) -> std::io::Result<()> {
    let p = path(dir);
    let old = match std::fs::read_to_string(&p) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(text) = &old
        && let Err(e) = toml::from_str::<TitleSettings>(text)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is not valid, so it is left alone: {e}", p.display()),
        ));
    }
    let line = format!("checked = {}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
    write_line(dir, old, "lyrics", "checked", &line)
}

/// Record the collection TMDB puts the film in `dir` in (`None`: in none), as
/// checked now. Same rules as `record_missing_checked`; a collection recorded
/// before is kept when TMDB now names none (the owner may delete it).
pub fn record_collection_checked(dir: &Path, found: Option<(i64, &str)>) -> std::io::Result<()> {
    let p = path(dir);
    let old = match std::fs::read_to_string(&p) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(text) = &old
        && let Err(e) = toml::from_str::<TitleSettings>(text)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is not valid, so it is left alone: {e}", p.display()),
        ));
    }
    let mut lines = vec![(
        "checked",
        format!("checked = {}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ")),
    )];
    if let Some((id, name)) = found {
        lines.push((
            "name",
            format!("name = {}", toml::Value::String(name.to_string())),
        ));
        lines.push(("tmdb", format!("tmdb = {id}")));
    }
    write_lines(dir, old, "collection", &lines)
}

/// Write `line` as `table.key` into the settings file (created if needed),
/// checking the result reads back.
fn write_line(
    dir: &Path,
    old: Option<String>,
    table: &str,
    key: &str,
    line: &str,
) -> std::io::Result<()> {
    write_lines(dir, old, table, &[(key, line.to_string())])
}

/// `write_line` for several keys of one table, in one write.
fn write_lines(
    dir: &Path,
    old: Option<String>,
    table: &str,
    lines: &[(&str, String)],
) -> std::io::Result<()> {
    let p = path(dir);
    let mut new = old.unwrap_or_else(|| HEADER.to_string());
    for (key, line) in lines {
        new = set_table_line(&new, table, key, line);
    }
    // Never write something that would not read back.
    toml::from_str::<TitleSettings>(&new).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}: {e}", p.display()),
        )
    })?;
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    std::fs::write(&tmp, new)?;
    std::fs::rename(&tmp, &p)
}

/// `text` with `line` as `[table]`'s `key`: replacing it, else added under
/// the table's header, else in a new table at the end.
fn set_table_line(text: &str, table: &str, key: &str, line: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = |l: &str| l.trim().starts_with('[');
    let is_artwork = |l: &str| {
        let t = l.trim();
        t.strip_prefix('[')
            .and_then(|r| r.split(']').next())
            .is_some_and(|name| name.trim() == table)
            && !t.starts_with("[[")
    };
    if let Some(start) = lines.iter().position(|l| is_artwork(l)) {
        let end = lines[start + 1..]
            .iter()
            .position(|l| header(l))
            .map_or(lines.len(), |i| start + 1 + i);
        let existing = (start + 1..end).find(|&i| {
            let t = lines[i].trim_start();
            t.strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        });
        match existing {
            Some(i) => lines[i] = line.to_string(),
            None => lines.insert(start + 1, line.to_string()),
        }
    } else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[{table}]"));
        lines.push(line.to_string());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
