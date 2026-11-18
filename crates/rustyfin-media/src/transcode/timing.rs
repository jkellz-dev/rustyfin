//! What a transcode must know about a file before ffmpeg runs: where its
//! clock starts, how long it is, whether ffmpeg will seek a little early
//! (B-frames), and, for copied video, where the keyframes are.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};

/// A keyframe scan reads the whole file; one that takes longer than this is
/// given up, and the video is encoded instead of copied.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Timings kept in memory; past this many the cache starts again.
const CACHE_SIZE: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceTiming {
    /// The file's start time: ffmpeg's `-ss` counts from it.
    pub start: f64,
    pub duration: f64,
    /// The video has B-frames, so ffmpeg seeks 3/23 s before an `-ss`.
    pub b_frames: bool,
    /// The video's keyframe times, when they were asked for.
    pub keyframes: Option<Vec<f64>>,
}

/// Read `ffprobe -of compact` output: `format|start_time=..|duration=..`,
/// `stream|has_b_frames=..` and, with `keyframes`, `packet|pts_time=..|flags=K..`.
pub fn parse_compact(text: &str, keyframes: bool) -> Option<SourceTiming> {
    let mut start = 0.0;
    let mut duration = None;
    let mut b_frames = false;
    let mut found = Vec::new();
    for line in text.lines() {
        let mut parts = line.trim().split('|');
        let section = parts.next().unwrap_or_default();
        let fields: HashMap<&str, &str> = parts.filter_map(|f| f.split_once('=')).collect();
        let number = |key: &str| fields.get(key).and_then(|v| v.parse::<f64>().ok());
        match section {
            "format" => {
                start = number("start_time").unwrap_or(0.0);
                duration = number("duration");
            }
            "stream" => b_frames |= number("has_b_frames").is_some_and(|b| b > 0.0),
            "packet" if keyframes => {
                if fields.get("flags").is_some_and(|f| f.starts_with('K'))
                    && let Some(t) = number("pts_time")
                {
                    found.push(t);
                }
            }
            _ => {}
        }
    }
    Some(SourceTiming {
        start,
        duration: duration.filter(|d| d.is_finite() && *d > 0.0)?,
        b_frames,
        keyframes: keyframes.then_some(found),
    })
}

/// ffprobe in the same directory as ffmpeg (a custom build ships both), else
/// from the PATH.
pub fn ffprobe_beside(ffmpeg: &str) -> String {
    let path = Path::new(ffmpeg);
    if path.file_name().and_then(|n| n.to_str()) != Some("ffmpeg") {
        return "ffprobe".to_string();
    }
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => {
            dir.join("ffprobe").to_string_lossy().into_owned()
        }
        _ => "ffprobe".to_string(),
    }
}

/// Read a file's timing with ffprobe.
pub async fn probe(ffprobe: &str, path: &Path, keyframes: bool) -> anyhow::Result<SourceTiming> {
    let entries = if keyframes {
        "format=start_time,duration:stream=has_b_frames:packet=pts_time,flags"
    } else {
        "format=start_time,duration:stream=has_b_frames"
    };
    let run = tokio::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-of",
            "compact",
            "-select_streams",
            "v:0",
            "-show_entries",
            entries,
        ])
        .arg(path)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(PROBE_TIMEOUT, run)
        .await
        .context("ffprobe took too long")?
        .context("failed to run ffprobe")?;
    if !output.status.success() {
        bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_compact(&String::from_utf8_lossy(&output.stdout), keyframes)
        .context("ffprobe gave no duration")
}

/// Timings by file, while the file is unchanged.
#[derive(Default)]
pub struct TimingCache {
    entries: Mutex<HashMap<(PathBuf, u64, Option<SystemTime>), Arc<SourceTiming>>>,
}

impl TimingCache {
    /// The file's timing, read once (again when keyframes are wanted and the
    /// cached one has none).
    pub async fn get(
        &self,
        ffprobe: &str,
        path: &Path,
        keyframes: bool,
    ) -> anyhow::Result<Arc<SourceTiming>> {
        let meta = tokio::fs::metadata(path)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        let key = (path.to_path_buf(), meta.len(), meta.modified().ok());
        let cached = self
            .entries
            .lock()
            .expect("timing cache")
            .get(&key)
            .cloned();
        if let Some(t) = cached
            && (!keyframes || t.keyframes.is_some())
        {
            return Ok(t);
        }
        let timing = Arc::new(probe(ffprobe, path, keyframes).await?);
        let mut entries = self.entries.lock().expect("timing cache");
        if entries.len() >= CACHE_SIZE {
            entries.clear();
        }
        entries.insert(key, timing.clone());
        Ok(timing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &str = "stream|has_b_frames=2\n\
        packet|pts_time=0.083000|flags=K__\n\
        packet|pts_time=0.041000|flags=___\n\
        packet|pts_time=4.005000|flags=K_\n\
        packet|pts_time=N/A|flags=K__\n\
        packet|pts_time=8.005000|flags=K__\n\
        format|start_time=0.000000|duration=120.040000\n";

    #[test]
    fn ffprobes_compact_output_gives_the_clock_and_the_keyframes() {
        let t = parse_compact(PROBE, true).unwrap();
        assert_eq!(t.start, 0.0);
        assert_eq!(t.duration, 120.04);
        assert!(t.b_frames, "has_b_frames=2");
        assert_eq!(t.keyframes, Some(vec![0.083, 4.005, 8.005]));
    }

    #[test]
    fn without_keyframes_asked_none_are_kept() {
        let t = parse_compact(PROBE, false).unwrap();
        assert_eq!(t.keyframes, None);
    }

    #[test]
    fn a_file_with_no_video_or_no_start_time_still_has_a_clock() {
        let t = parse_compact("format|start_time=N/A|duration=200.5\n", false).unwrap();
        assert_eq!(t.start, 0.0);
        assert_eq!(t.duration, 200.5);
        assert!(!t.b_frames);
        let t = parse_compact(
            "stream|has_b_frames=0\nformat|start_time=1.400000|duration=60\n",
            true,
        )
        .unwrap();
        assert_eq!(t.start, 1.4);
        assert!(!t.b_frames);
        assert_eq!(t.keyframes, Some(vec![]), "asked for, none found");
    }

    #[test]
    fn without_a_duration_there_is_no_timeline() {
        assert!(parse_compact("format|start_time=0|duration=N/A\n", false).is_none());
        assert!(parse_compact("", false).is_none());
    }

    #[test]
    fn ffprobe_sits_beside_ffmpeg() {
        assert_eq!(ffprobe_beside("/usr/bin/ffmpeg"), "/usr/bin/ffprobe");
        assert_eq!(ffprobe_beside("ffmpeg"), "ffprobe");
        assert_eq!(
            ffprobe_beside("/opt/jellyfin-ffmpeg/ffmpeg"),
            "/opt/jellyfin-ffmpeg/ffprobe"
        );
        // A wrapper of another name: ffprobe from the PATH.
        assert_eq!(ffprobe_beside("/tests/fake-ffmpeg/runs.sh"), "ffprobe");
    }
}
