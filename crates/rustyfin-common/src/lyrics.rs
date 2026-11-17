//! Track lyrics, read the way Jellyfin 10.10 does: a sidecar file next to the
//! track with the same name (`.lrc`, `.elrc`, then `.txt`), else lyrics
//! embedded in the file's tags. Nothing is fetched online.

use std::path::{Path, PathBuf};

/// Ticks (100 ns) per millisecond, as Jellyfin counts time.
const TICKS_PER_MS: i64 = 10_000;

/// One line of lyrics; `start_ticks` is set for synced (LRC) lyrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub text: String,
    pub start_ticks: Option<i64>,
}

/// The sidecar lyrics file for a track, if there is one.
pub fn sidecar_path(track: &Path) -> Option<PathBuf> {
    ["lrc", "elrc", "txt"]
        .iter()
        .map(|ext| track.with_extension(ext))
        .find(|p| p.is_file())
}

/// The track's lyrics: its sidecar file, else the lyrics in its tags.
pub fn read_lyrics(track: &Path) -> Option<Vec<LyricLine>> {
    if let Some(sidecar) = sidecar_path(track) {
        let bytes = std::fs::read(&sidecar).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let lines = if sidecar.extension().is_some_and(|e| e == "txt") {
            parse_txt(&text)
        } else {
            parse_lrc(&text)
        };
        return Some(lines);
    }
    let embedded = embedded_lyrics(track)?;
    let synced = parse_lrc(&embedded);
    Some(if synced.is_empty() {
        parse_txt(&embedded)
    } else {
        synced
    })
}

fn embedded_lyrics(track: &Path) -> Option<String> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::ItemKey;
    let file = lofty::read_from_path(track).ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag())?;
    let text = tag.get_string(ItemKey::Lyrics)?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Plain text: every line, blank ones included, none timed.
pub fn parse_txt(text: &str) -> Vec<LyricLine> {
    let text = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    text.split('\n')
        .map(|line| LyricLine {
            text: line.to_string(),
            start_ticks: None,
        })
        .collect()
}

/// LRC: one line per timestamp (`[mm:ss.xx]`, several allowed per line), in
/// time order; metadata tags (`[ar:...]`), blank and untimed lines are dropped.
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = line.trim();
        let mut starts = Vec::new();
        while let Some(tag_end) = rest.strip_prefix('[').and_then(|r| r.find(']')) {
            let tag = &rest[1..=tag_end];
            match parse_timestamp(tag) {
                Some(ms) => starts.push(ms * TICKS_PER_MS),
                None => break,
            }
            rest = rest[tag_end + 2..].trim_start();
        }
        for start in starts {
            out.push(LyricLine {
                text: rest.trim_end().to_string(),
                start_ticks: Some(start),
            });
        }
    }
    out.sort_by_key(|l| l.start_ticks);
    out
}

/// `mm:ss`, `mm:ss.f`, `mm:ss.ff`, `mm:ss.fff` (or `:` before the fraction) to ms.
fn parse_timestamp(tag: &str) -> Option<i64> {
    let (min, rest) = tag.split_once(':')?;
    let (sec, frac) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(min)
        || !all_digits(sec)
        || !(frac.is_empty() || all_digits(frac))
        || frac.len() > 3
    {
        return None;
    }
    let mut ms = min.parse::<i64>().ok()? * 60_000 + sec.parse::<i64>().ok()? * 1000;
    if !frac.is_empty() {
        ms += frac.parse::<i64>().ok()? * 10_i64.pow(3 - frac.len() as u32);
    }
    Some(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed(text: &str, ms: i64) -> LyricLine {
        LyricLine {
            text: text.to_string(),
            start_ticks: Some(ms * TICKS_PER_MS),
        }
    }

    #[test]
    fn lrc_lines_are_timed_sorted_and_repeated_per_timestamp() {
        let lrc = "\u{feff}[ar:Someone]\n[length:03:00]\n[00:10.5]Later\n[00:01.25] First \nuntimed\n\n[00:02][00:20.125]Both\n";
        assert_eq!(
            parse_lrc(lrc),
            vec![
                timed("First", 1250),
                timed("Both", 2000),
                timed("Later", 10_500),
                timed("Both", 20_125)
            ]
        );
    }

    #[test]
    fn timestamps_take_one_to_three_fraction_digits() {
        assert_eq!(parse_timestamp("01:02.5"), Some(62_500));
        assert_eq!(parse_timestamp("01:02.50"), Some(62_500));
        assert_eq!(parse_timestamp("01:02.500"), Some(62_500));
        assert_eq!(parse_timestamp("01:02:05"), Some(62_050));
        assert_eq!(parse_timestamp("ar:Someone"), None);
        assert_eq!(parse_timestamp("01:02.5000"), None);
    }

    #[test]
    fn txt_keeps_every_line() {
        let lines: Vec<String> = parse_txt("a\r\n\r\nb\n")
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(lines, ["a", "", "b", ""]);
    }

    #[test]
    fn the_sidecar_is_lrc_before_txt() {
        let dir = std::env::temp_dir().join(format!("rustyfin-lyrics-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let track = dir.join("Song.flac");
        assert_eq!(sidecar_path(&track), None);
        std::fs::write(dir.join("Song.txt"), "x").unwrap();
        assert_eq!(sidecar_path(&track), Some(dir.join("Song.txt")));
        std::fs::write(dir.join("Song.lrc"), "x").unwrap();
        assert_eq!(sidecar_path(&track), Some(dir.join("Song.lrc")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
