//! Subtitle files beside a video (Bazarr, downloads, rips), named as Jellyfin
//! reads them: `<video name>[.<language>][.forced|.default|.sdh].<srt|vtt|ass|ssa>`.

use std::path::{Path, PathBuf};

/// One subtitle file found beside a video.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleFile {
    pub path: PathBuf,
    /// ffprobe's codec name: `subrip`, `webvtt`, `ass` or `ssa`.
    pub codec: &'static str,
    pub language: Option<String>,
    pub forced: bool,
    pub default: bool,
    pub hearing_impaired: bool,
}

/// The subtitle files among `names` (the video's folder) that belong to
/// `video`, sorted by file name.
pub fn subtitle_files(video: &Path, names: &[String]) -> Vec<SubtitleFile> {
    let (Some(stem), Some(dir)) = (video.file_stem().and_then(|s| s.to_str()), video.parent())
    else {
        return Vec::new();
    };
    let stem = stem.to_lowercase();
    let mut found: Vec<SubtitleFile> = names
        .iter()
        .filter_map(|name| {
            let lower = name.to_lowercase();
            let (rest, ext) = lower.rsplit_once('.')?;
            let codec = match ext {
                "srt" => "subrip",
                "vtt" => "webvtt",
                "ass" => "ass",
                "ssa" => "ssa",
                _ => return None,
            };
            let tags = if rest == stem {
                ""
            } else {
                rest.strip_prefix(&stem)?.strip_prefix('.')?
            };
            let mut file = SubtitleFile {
                path: dir.join(name),
                codec,
                language: None,
                forced: false,
                default: false,
                hearing_impaired: false,
            };
            for tag in tags.split('.').filter(|t| !t.is_empty()) {
                match tag {
                    "forced" | "foreign" => file.forced = true,
                    "default" => file.default = true,
                    "sdh" | "cc" | "hi" => file.hearing_impaired = true,
                    lang if file.language.is_none()
                        && lang.len() <= 3
                        && lang.chars().all(|c| c.is_ascii_alphabetic()) =>
                    {
                        file.language = Some(lang.to_string());
                    }
                    _ => {}
                }
            }
            Some(file)
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// Subtitle files beside `video`, read from its folder.
pub fn find_subtitle_files(video: &Path) -> Vec<SubtitleFile> {
    let Some(dir) = video.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    subtitle_files(video, &names)
}

/// SubRip text as WebVTT (what browsers read): a header, and `.` instead of
/// `,` before the milliseconds in cue times.
pub fn srt_to_vtt(srt: &str) -> String {
    let text = srt
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut out = String::from("WEBVTT\n\n");
    for line in text.lines() {
        if line.contains("-->") {
            // 00:00:01,000 --> 00:00:02,500: only the times change.
            out.push_str(&line.replace(',', "."));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File name, codec, language, forced, default, hearing impaired.
    type Summary = (String, &'static str, Option<String>, bool, bool, bool);

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn files_named_after_the_video_are_its_subtitles() {
        let video = Path::new("/m/Film (2020)/Film (2020).mkv");
        let found = subtitle_files(
            video,
            &names(&[
                "Film (2020).mkv",
                "Film (2020).nfo",
                "Film (2020).en.srt",
                "Film (2020).srt",
                "Film (2020).fr.forced.srt",
                "Film (2020).eng.sdh.ass",
                "Film (2020).de.default.vtt",
                "Other Film.en.srt",
                "Film (2020)-trailer.en.srt",
                "Film (2020).en.sub",
            ]),
        );
        let summary: Vec<Summary> = found
            .iter()
            .map(|s| {
                (
                    s.path.file_name().unwrap().to_string_lossy().to_string(),
                    s.codec,
                    s.language.clone(),
                    s.forced,
                    s.default,
                    s.hearing_impaired,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "Film (2020).de.default.vtt".into(),
                    "webvtt",
                    Some("de".into()),
                    false,
                    true,
                    false
                ),
                (
                    "Film (2020).en.srt".into(),
                    "subrip",
                    Some("en".into()),
                    false,
                    false,
                    false
                ),
                (
                    "Film (2020).eng.sdh.ass".into(),
                    "ass",
                    Some("eng".into()),
                    false,
                    false,
                    true
                ),
                (
                    "Film (2020).fr.forced.srt".into(),
                    "subrip",
                    Some("fr".into()),
                    true,
                    false,
                    false
                ),
                (
                    "Film (2020).srt".into(),
                    "subrip",
                    None,
                    false,
                    false,
                    false
                ),
            ]
        );
        assert_eq!(
            found[0].path,
            Path::new("/m/Film (2020)/Film (2020).de.default.vtt")
        );
    }

    #[test]
    fn names_compare_without_case_and_foreign_means_forced() {
        let video = Path::new("/tv/Show/Season 01/Show S01E01.mp4");
        let found = subtitle_files(
            video,
            &names(&["show s01e01.EN.FOREIGN.SRT", "Show S01E02.en.srt"]),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].language.as_deref(), Some("en"));
        assert!(found[0].forced);
    }

    #[test]
    fn subrip_becomes_webvtt() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello, there\r\n\r\n2\r\n00:01:00,250 --> 00:01:02,000\r\nBye\r\n";
        let vtt = srt_to_vtt(srt);
        assert!(vtt.starts_with("WEBVTT\n\n"), "{vtt}");
        assert!(vtt.contains("00:00:01.000 --> 00:00:02.500"), "{vtt}");
        assert!(vtt.contains("00:01:00.250 --> 00:01:02.000"), "{vtt}");
        assert!(
            vtt.contains("Hello, there"),
            "commas in the text stay: {vtt}"
        );
        assert!(!vtt.contains('\r'));
        // A byte-order mark at the start is dropped.
        assert!(srt_to_vtt("\u{feff}1\n00:00:01,000 --> 00:00:02,000\nA\n").starts_with("WEBVTT"));
    }
}
