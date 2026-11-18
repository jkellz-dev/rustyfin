use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::command::build_ffmpeg_args;
use super::job::TranscodeJob;
use super::params::StreamParams;

/// Request to start a new transcode job.
pub struct TranscodeRequest {
    pub play_session_id: String,
    pub item_id: Uuid,
    pub device_id: String,
    pub user_id: Uuid,
    pub params: StreamParams,
}

pub struct TranscodeManager {
    jobs: RwLock<HashMap<String, TranscodeJob>>,
    pub ffmpeg_path: String,
    pub temp_base: PathBuf,
    pub hardware_acceleration: String,
    pub max_concurrent: usize,
    pub segment_duration: u32,
    pub threads_per_job: u32,
}

impl TranscodeManager {
    pub fn new(
        ffmpeg_path: String,
        temp_dir: Option<String>,
        hardware_acceleration: String,
        max_concurrent: usize,
        segment_duration: u32,
        threads_per_job: u32,
    ) -> Self {
        let temp_base = temp_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("rustyfin-transcode"));

        info!(
            ffmpeg = %ffmpeg_path,
            temp = %temp_base.display(),
            hw = %hardware_acceleration,
            max_concurrent,
            "TranscodeManager initialized"
        );

        Self {
            jobs: RwLock::new(HashMap::new()),
            ffmpeg_path,
            temp_base,
            hardware_acceleration,
            max_concurrent,
            segment_duration,
            threads_per_job,
        }
    }

    /// Start a new transcode job, spawning ffmpeg and waiting for the first segment.
    pub async fn start_transcode(
        self: &Arc<Self>,
        request: TranscodeRequest,
    ) -> anyhow::Result<String> {
        // Check concurrency limit
        if self.at_capacity().await {
            bail!(
                "transcode concurrency limit reached ({} active)",
                self.max_concurrent
            );
        }

        // The session id becomes a directory under `temp_base` that is created,
        // written by ffmpeg and later removed recursively, so an id that could leave
        // `temp_base` (a separator, `..`, an absolute path) must never get this far.
        // The API checks it first; this guards every other caller.
        if !super::names::is_valid_session_id(&request.play_session_id) {
            bail!("invalid play session id");
        }

        // Kill any existing job for this session
        self.kill_job(&request.play_session_id).await;

        // Create output directory
        let output_dir = self.temp_base.join(&request.play_session_id);
        tokio::fs::create_dir_all(&output_dir)
            .await
            .context("failed to create transcode output directory")?;

        // Build ffmpeg args
        let args = build_ffmpeg_args(&request.params);

        info!(
            play_session_id = %request.play_session_id,
            item_id = %request.item_id,
            args = ?args,
            "starting ffmpeg transcode"
        );

        // Spawn ffmpeg process
        let mut child = Command::new(&self.ffmpeg_path)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("failed to spawn ffmpeg")?;

        // Take stderr for progress monitoring
        let stderr = child.stderr.take();

        let now = chrono::Utc::now();
        let segment_duration = request.params.segment_duration;
        let offset_ticks = request.params.start_number as i64 * segment_duration as i64 * TICKS;
        let session_start_ticks = request.params.start_time_ticks.unwrap_or(0) - offset_ticks;
        let job = TranscodeJob {
            id: Uuid::new_v4(),
            play_session_id: request.play_session_id.clone(),
            item_id: request.item_id,
            device_id: request.device_id,
            user_id: request.user_id,
            output_dir: output_dir.clone(),
            process: Some(child),
            started_at: now,
            last_segment_request: now,
            transcoding_position_ticks: offset_ticks,
            offset_ticks,
            session_start_ticks,
            player_ticks: offset_ticks,
            kill_timer: None,
            segment_duration,
            stream_params_cache: Some(request.params),
            is_paused: false,
        };

        let play_session_id = request.play_session_id.clone();
        self.insert_job(play_session_id.clone(), job).await;

        // Spawn progress monitoring task
        if let Some(stderr) = stderr {
            let manager = Arc::clone(self);
            let session_id = play_session_id.clone();
            tokio::spawn(async move {
                monitor_progress(&manager, &session_id, stderr).await;
            });
        }

        // Wait for first segment to appear (up to 30 seconds)
        let first_segment = output_dir.join("0.ts");
        let alt_segment = output_dir.join("0.m4s");
        let playlist = output_dir.join("playlist.m3u8");

        let wait_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            wait_for_any_file(&[&first_segment, &alt_segment, &playlist]),
        )
        .await;

        match wait_result {
            Ok(()) => {
                info!(
                    play_session_id = %play_session_id,
                    "first transcode segment ready"
                );
            }
            Err(_) => {
                warn!(
                    play_session_id = %play_session_id,
                    "timeout waiting for first segment, continuing anyway"
                );
            }
        }

        Ok(play_session_id)
    }

    /// Restart a transcode at a specific position (for seeking).
    pub async fn restart_at(
        self: &Arc<Self>,
        play_session_id: &str,
        start_ticks: i64,
        start_number: u32,
    ) -> anyhow::Result<()> {
        let old_job = self.remove_job(play_session_id).await;
        if let Some(mut job) = old_job {
            // Stop the old encoder but keep the output dir for existing segments
            job.stop_process().await;
            if let Some(timer) = job.kill_timer.take() {
                timer.abort();
            }

            // Build new params with updated start position
            let mut params = job.stream_params_cache.unwrap_or_else(|| {
                warn!("no cached stream params for restart, cannot seek");
                StreamParams {
                    input_path: PathBuf::new(),
                    output_dir: job.output_dir.clone(),
                    start_time_ticks: Some(start_ticks),
                    video_codec: super::params::VideoCodec::H264,
                    video_tag: None,
                    video_bitrate: None,
                    max_width: None,
                    max_height: None,
                    max_framerate: None,
                    audio_codec: super::params::AudioCodec::AAC,
                    audio_bitrate: None,
                    audio_channels: None,
                    audio_sample_rate: None,
                    audio_stream_index: None,
                    subtitle_mode: super::params::SubtitleMode::None,
                    subtitle_stream_index: None,
                    segment_duration: self.segment_duration,
                    segment_format: super::params::SegmentFormat::MpegTs,
                    start_number: 0,
                    hardware_accel: super::params::HardwareAccel::None,
                    threads: 0,
                    audio_only: false,
                }
            });
            params.start_time_ticks = Some(start_ticks);
            params.start_number = start_number;

            let request = TranscodeRequest {
                play_session_id: play_session_id.to_string(),
                item_id: job.item_id,
                device_id: job.device_id.clone(),
                user_id: job.user_id,
                params,
            };

            self.start_transcode(request).await?;
        }

        Ok(())
    }

    /// Restart the session's transcode at a segment (a seek the encoder has not
    /// reached).
    pub async fn restart_at_segment(
        self: &Arc<Self>,
        play_session_id: &str,
        segment_id: u32,
    ) -> anyhow::Result<()> {
        let Some(session_start) = self
            .jobs
            .read()
            .await
            .get(play_session_id)
            .map(|j| j.session_start_ticks)
        else {
            return Ok(());
        };
        let Some(at) = self.segment_ticks(play_session_id, segment_id).await else {
            return Ok(());
        };
        self.restart_at(play_session_id, session_start + at, segment_id)
            .await?;
        // The new encoder's first segment sits where the playlist placed it,
        // which for copied video is not `segment_id` × the segment length.
        if let Some(job) = self.jobs.write().await.get_mut(play_session_id) {
            job.offset_ticks = at;
            job.session_start_ticks = session_start;
            job.player_ticks = at;
            job.transcoding_position_ticks = job.transcoding_position_ticks.max(at);
        }
        Ok(())
    }

    /// Where segment `n` starts on the session clock: from the playlist when
    /// it lists the segment (copied video ends segments on the source's
    /// keyframes, so they are uneven), else `n` × the segment length.
    async fn segment_ticks(&self, play_session_id: &str, segment_id: u32) -> Option<i64> {
        let (dir, offset, duration) = {
            let jobs = self.jobs.read().await;
            let job = jobs.get(play_session_id)?;
            (
                job.output_dir.clone(),
                job.offset_ticks,
                job.segment_duration,
            )
        };
        let listed = tokio::fs::read_to_string(dir.join("playlist.m3u8"))
            .await
            .ok()
            .and_then(|p| listed_segment_start(&p, segment_id));
        Some(match listed {
            Some(secs) => offset + (secs * TICKS as f64).round() as i64,
            None => segment_start_ticks(segment_id, duration),
        })
    }

    /// Whether a missing segment needs the encoder restarted at it, rather than
    /// waiting for the running encoder to reach it: it lies before this
    /// encoder's first segment (nothing will write it), or more than two
    /// minutes past what it has encoded.
    pub async fn needs_restart(&self, play_session_id: &str, segment_id: u32) -> bool {
        let Some(start) = self.segment_ticks(play_session_id, segment_id).await else {
            return false;
        };
        let jobs = self.jobs.read().await;
        let Some(job) = jobs.get(play_session_id) else {
            return false;
        };
        start < job.offset_ticks || start > job.transcoding_position_ticks + SEEK_AHEAD_TICKS
    }

    /// Record a segment request: the player is at that segment. Resumes a
    /// throttled encoder.
    pub async fn touch_segment(&self, play_session_id: &str, segment_id: u32) {
        if let Some(at) = self.segment_ticks(play_session_id, segment_id).await {
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.get_mut(play_session_id) {
                job.player_ticks = at;
            }
        }
        self.touch_job(play_session_id).await;
    }

    /// Get a reference to a job by play session ID.
    pub async fn get_job_output_dir(&self, play_session_id: &str) -> Option<PathBuf> {
        let jobs = self.jobs.read().await;
        jobs.get(play_session_id).map(|j| j.output_dir.clone())
    }

    /// Whether the session has a usable transcode: ffmpeg running, or finished
    /// with every segment on disk (see [`TranscodeJob::is_usable`]).
    pub async fn has_active_job(&self, play_session_id: &str) -> bool {
        let mut jobs = self.jobs.write().await;
        jobs.get_mut(play_session_id)
            .is_some_and(|job| job.is_usable())
    }

    /// Update the last segment request timestamp for a job.
    /// Also resumes a throttled job if it was paused.
    pub async fn touch_job(&self, play_session_id: &str) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(play_session_id) {
            job.last_segment_request = chrono::Utc::now();
            // Resume if paused — client is actively requesting segments
            if job.is_paused {
                job.resume();
                job.is_paused = false;
            }
        }
    }

    /// Get the transcoding position (in ticks) for a job.
    pub async fn get_transcoding_position(&self, play_session_id: &str) -> Option<i64> {
        let jobs = self.jobs.read().await;
        jobs.get(play_session_id)
            .map(|j| j.transcoding_position_ticks)
    }

    /// Get the segment duration for a job.
    pub async fn get_segment_duration(&self, play_session_id: &str) -> Option<u32> {
        let jobs = self.jobs.read().await;
        jobs.get(play_session_id).map(|j| j.segment_duration)
    }

    /// Kill a specific job by play session ID.
    pub async fn kill_job(&self, play_session_id: &str) {
        // Out of the map first: stopping ffmpeg can take a moment, and holding
        // the lock meanwhile would stall every other transcode request.
        let removed = self.jobs.write().await.remove(play_session_id);
        if let Some(mut job) = removed {
            info!(play_session_id, "killing transcode job");
            job.kill().await;
        }
    }

    /// Remove the matching jobs under the lock, then stop them outside it.
    async fn kill_where(&self, why: &str, matches: impl Fn(&TranscodeJob) -> bool) {
        let removed: Vec<(String, TranscodeJob)> = {
            let mut jobs = self.jobs.write().await;
            let keys: Vec<String> = jobs
                .iter()
                .filter(|(_, j)| matches(j))
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| jobs.remove(&k).map(|j| (k, j)))
                .collect()
        };
        for (key, mut job) in removed {
            info!(play_session_id = %key, "killing transcode job ({why})");
            job.kill().await;
        }
    }

    /// Kill all jobs for a given device ID.
    pub async fn kill_jobs_for_device(&self, device_id: &str) {
        self.kill_where("device", |j| j.device_id == device_id)
            .await;
    }

    /// Kill the jobs on a device that belong to `user_id`.
    pub async fn kill_jobs_for_user_device(&self, device_id: &str, user_id: Uuid) {
        self.kill_where("device", |j| {
            j.device_id == device_id && j.user_id == user_id
        })
        .await;
    }

    /// Throttle ffmpeg processes that are too far ahead of playback.
    ///
    /// On a fast machine, ffmpeg can transcode much faster than real-time.
    /// To avoid wasting CPU and disk, pause it when it's >120s ahead
    /// and resume when it's <30s ahead.
    pub async fn throttle_jobs(&self) {
        let throttle_ahead = 120 * TICKS;
        let resume_threshold = 30 * TICKS;

        let mut jobs = self.jobs.write().await;
        for job in jobs.values_mut() {
            if !job.is_alive() {
                continue;
            }
            // How far the encoder is past the segment the player last asked for.
            let ahead = job.transcoding_position_ticks - job.player_ticks;
            if ahead > throttle_ahead && !job.is_paused {
                job.pause();
                job.is_paused = true;
            } else if ahead <= resume_threshold && job.is_paused {
                job.resume();
                job.is_paused = false;
            }
        }
    }

    /// Kill stale jobs that haven't had a segment request in the given duration,
    /// and jobs whose encoder failed or was killed. One that finished the file
    /// stays until the player stops asking for its segments.
    pub async fn cleanup_stale_jobs(&self) {
        let stale_threshold = chrono::Duration::seconds(60);
        let now = chrono::Utc::now();
        let removed: Vec<(String, TranscodeJob)> = {
            let mut jobs = self.jobs.write().await;
            let keys: Vec<String> = jobs
                .iter_mut()
                .filter_map(|(k, j)| {
                    (now - j.last_segment_request > stale_threshold || !j.is_usable())
                        .then(|| k.clone())
                })
                .collect();
            keys.into_iter()
                .filter_map(|k| jobs.remove(&k).map(|j| (k, j)))
                .collect()
        };
        for (key, mut job) in removed {
            warn!(play_session_id = %key, "cleaning up stale transcode job");
            job.kill().await;
        }
    }

    /// Get the set of active job play-session IDs (for temp cleanup).
    pub async fn active_job_ids(&self) -> std::collections::HashSet<String> {
        self.jobs.read().await.keys().cloned().collect()
    }

    /// Number of currently active transcode jobs.
    pub async fn active_job_count(&self) -> usize {
        self.jobs.read().await.len()
    }

    /// Kill all active jobs (used during shutdown).
    pub async fn kill_all(&self) {
        self.kill_where("server stopping", |_| true).await;
    }

    /// The user who started the job for this play session, if there is one.
    ///
    /// A job exists from the moment it is registered, whether or not its ffmpeg
    /// process is still alive, so this is the right question for "whose session is
    /// this"; `has_active_job` is about the process.
    pub async fn job_owner(&self, play_session_id: &str) -> Option<Uuid> {
        let jobs = self.jobs.read().await;
        jobs.get(play_session_id).map(|j| j.user_id)
    }

    /// Insert a job into the manager.
    pub async fn insert_job(&self, play_session_id: String, job: TranscodeJob) {
        let mut jobs = self.jobs.write().await;
        jobs.insert(play_session_id, job);
    }

    /// Remove and return a job (e.g. for restarting during seek).
    pub(crate) async fn remove_job(&self, play_session_id: &str) -> Option<TranscodeJob> {
        let mut jobs = self.jobs.write().await;
        jobs.remove(play_session_id)
    }

    /// Check if adding a new job would exceed the concurrency limit.
    pub async fn at_capacity(&self) -> bool {
        if self.max_concurrent == 0 {
            return false; // unlimited
        }
        // Count running encoders only: a finished session holds no ffmpeg.
        let mut jobs = self.jobs.write().await;
        jobs.values_mut()
            .map(TranscodeJob::is_alive)
            .filter(|alive| *alive)
            .count()
            >= self.max_concurrent
    }

    /// Update the transcoding position for a job.
    pub(crate) async fn update_position(&self, play_session_id: &str, position_ticks: i64) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(play_session_id) {
            // ffmpeg runs with -copyts, so it reports file time; segment 0
            // sits at the session's start in the file.
            job.transcoding_position_ticks = position_ticks - job.session_start_ticks;
        }
    }
}

/// Parse ffmpeg stderr output to track transcoding progress.
async fn monitor_progress(
    manager: &TranscodeManager,
    play_session_id: &str,
    stderr: tokio::process::ChildStderr,
) {
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        // ffmpeg progress lines look like:
        // frame=  120 fps= 60 q=23.0 size=    1024kB time=00:00:05.00 bitrate=1677.7kbits/s speed=2.50x
        if let Some(time_pos) = line.find("time=") {
            let time_str = &line[time_pos + 5..];
            if let Some(end) = time_str.find(' ') {
                let timestamp = &time_str[..end];
                if let Some(ticks) = parse_ffmpeg_time(timestamp) {
                    manager.update_position(play_session_id, ticks).await;
                    debug!(
                        play_session_id,
                        position_ticks = ticks,
                        "transcode progress"
                    );
                }
            }
        }

        // Log ffmpeg errors
        if line.contains("Error") || line.contains("error") {
            error!(play_session_id, line = %line, "ffmpeg error");
        }
    }

    info!(play_session_id, "ffmpeg progress monitor ended");
}

/// 100ns ticks per second.
const TICKS: i64 = 10_000_000;

/// A missing segment further than this past the encoder restarts it there.
const SEEK_AHEAD_TICKS: i64 = 120 * TICKS;

/// Where segment `n` starts on the session clock.
fn segment_start_ticks(segment_id: u32, segment_duration: u32) -> i64 {
    segment_id as i64 * segment_duration as i64 * TICKS
}

/// Seconds from the first segment a playlist lists to the start of segment
/// `n` (segments are named `<n>.ts` / `<n>.m4s`), or `None` when the playlist
/// does not reach it yet or starts after it. The segment just past the last
/// listed one starts where that one ends.
fn listed_segment_start(playlist: &str, n: u32) -> Option<f64> {
    let mut at = 0.0;
    let mut length = None;
    let mut first = None;
    let mut next = None;
    for line in playlist.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            length = rest.split(',').next()?.trim().parse::<f64>().ok();
        } else if !line.is_empty() && !line.starts_with('#') {
            let name = line.rsplit('/').next()?;
            let number: u32 = name.split('.').next()?.parse().ok()?;
            first.get_or_insert(number);
            if number == n {
                return Some(at);
            }
            at += length.take()?;
            next = Some(number + 1);
        }
    }
    (next == Some(n) && first.is_some_and(|f| f <= n)).then_some(at)
}

/// Parse ffmpeg timestamp "HH:MM:SS.ms" to 100ns ticks.
fn parse_ffmpeg_time(timestamp: &str) -> Option<i64> {
    let parts: Vec<&str> = timestamp.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;

    let total_seconds = hours * 3600.0 + minutes * 60.0 + seconds;
    Some((total_seconds * 10_000_000.0) as i64)
}

/// Wait until any of the given files exists.
async fn wait_for_any_file(paths: &[&std::path::Path]) {
    loop {
        for path in paths {
            if path.exists() {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sessions {
        use super::super::*;
        use crate::transcode::params::{
            AudioCodec, HardwareAccel, SegmentCut, SegmentFormat, SubtitleMode, VideoCodec,
        };
        use crate::transcode::timeline::Timeline;
        use std::path::Path;
        use std::time::Duration;

        const SEC: i64 = 10_000_000;

        /// A stand-in for ffmpeg from `tests/fixtures/fake-ffmpeg`. They are
        /// committed files: writing a script at run time and executing it
        /// while a parallel test forks fails now and then with "Text file busy".
        fn fake(name: &str) -> String {
            format!(
                "{}/tests/fixtures/fake-ffmpeg/{name}.sh",
                env!("CARGO_MANIFEST_DIR")
            )
        }

        fn root(name: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("rf-session-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn manager(root: &Path, ffmpeg: String) -> Arc<TranscodeManager> {
            Arc::new(TranscodeManager::new(
                ffmpeg,
                Some(root.join("t").to_string_lossy().into_owned()),
                "none".into(),
                4,
                6,
                0,
            ))
        }

        fn params(video_codec: VideoCodec, cut: SegmentCut) -> StreamParams {
            StreamParams {
                input_path: PathBuf::from("/media/film.mkv"),
                output_dir: PathBuf::new(),
                start_time_ticks: None,
                video_codec,
                video_tag: None,
                video_bitrate: None,
                max_width: None,
                max_height: None,
                max_framerate: None,
                audio_codec: AudioCodec::AAC,
                audio_bitrate: None,
                audio_channels: Some(2),
                audio_sample_rate: Some(48000),
                audio_stream_index: None,
                subtitle_mode: SubtitleMode::None,
                subtitle_stream_index: None,
                segment_duration: 6,
                segment_format: SegmentFormat::MpegTs,
                start_number: 0,
                hardware_accel: HardwareAccel::None,
                threads: 0,
                audio_only: false,
                run: 0,
                cut,
                keyframes_from: None,
            }
        }

        /// An encoded two-hour film: a segment every 6 s.
        fn encoded() -> Plan {
            Plan {
                timeline: Arc::new(Timeline::grid(0.0, 7200.0, 6)),
                params: params(VideoCodec::H264, SegmentCut::Every),
                source_start: 0.0,
                b_frames: false,
            }
        }

        /// Copied video whose keyframes are uneven (7.5 s, 8 s, 6.5 s, 9 s, 7 s).
        fn copied(b_frames: bool) -> Plan {
            let keyframes = [0.0, 7.5, 15.5, 22.0, 31.0, 38.0];
            Plan {
                timeline: Arc::new(Timeline::keyframes(&keyframes, 44.0, 6).unwrap()),
                params: params(VideoCodec::Copy, SegmentCut::At(vec![])),
                source_start: 0.0,
                b_frames,
            }
        }

        fn request(session: &str, plan: Plan, start_segment: u32) -> TranscodeRequest {
            TranscodeRequest {
                play_session_id: session.into(),
                item_id: Uuid::nil(),
                device_id: "d".into(),
                user_id: Uuid::nil(),
                plan,
                start_segment,
            }
        }

        /// The fake ffmpeg's command lines, once `n` runs have logged.
        async fn calls(root: &Path, n: usize) -> Vec<String> {
            let mut calls = String::new();
            for _ in 0..50 {
                calls = std::fs::read_to_string(root.join("calls.log")).unwrap_or_default();
                if calls.lines().count() >= n {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            calls.lines().map(str::to_string).collect()
        }

        async fn wait_exit(tm: &TranscodeManager, session: &str) {
            for _ in 0..40 {
                let alive = {
                    let mut jobs = tm.jobs.write().await;
                    jobs.get_mut(session).is_some_and(|j| j.is_alive())
                };
                if !alive {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("fake ffmpeg did not exit");
        }

        // An encoder that reached the end of the file (exit 0) leaves a complete
        // session: the player is still reading its segments, so a playlist
        // reload must not start it again and the cleanup must not delete it.
        #[tokio::test]
        async fn a_finished_transcode_stays_until_the_player_leaves() {
            let root = root("finished");
            let tm = manager(&root, fake("finishes"));
            tm.start_transcode(request("s1", encoded(), 0))
                .await
                .unwrap();
            wait_exit(&tm, "s1").await;

            assert!(
                tm.has_active_job("s1").await,
                "a finished session is still usable"
            );
            tm.cleanup_stale_jobs().await;
            let segment = tm
                .wait_for_segment("s1", 0, Duration::from_secs(3))
                .await
                .expect("kept by the cleanup");
            assert!(segment.exists(), "its segments are kept");
        }

        #[tokio::test]
        async fn a_failed_transcode_is_cleaned_up() {
            let root = root("failed");
            let tm = manager(&root, fake("fails"));
            tm.start_transcode(request("s2", encoded(), 0))
                .await
                .unwrap();
            wait_exit(&tm, "s2").await;

            assert!(!tm.has_active_job("s2").await);
            tm.cleanup_stale_jobs().await;
            assert!(tm.get_job_output_dir("s2").await.is_none());
        }

        // The playlist lists the whole film from the first request; ffmpeg
        // starts at the segment that holds the resume point (41:06 for 41:10),
        // with keyframes forced on the segment marks from there.
        #[tokio::test]
        async fn a_resumed_session_lists_the_whole_film_and_starts_at_its_segment() {
            let root = root("resume");
            let tm = manager(&root, fake("runs"));
            tm.start_transcode(request("s3", encoded(), 411))
                .await
                .unwrap();

            let playlist = tm.playlist("s3").await.unwrap();
            assert!(playlist.contains("#EXT-X-PLAYLIST-TYPE:VOD"), "{playlist}");
            assert!(playlist.contains("\n0.ts\n"), "from the start: {playlist}");
            assert!(playlist.contains("\n1199.ts\n#EXT-X-ENDLIST"), "{playlist}");

            let first = &calls(&root, 1).await[0];
            assert!(first.contains("-ss 2466.000"), "{first}");
            assert!(first.contains("-start_number 411"), "{first}");
            assert!(
                first.contains("-force_key_frames expr:gte(t,2466.000000+n_forced*6)"),
                "{first}"
            );
            tm.kill_all().await;
        }

        // A copied stream's restart lands on the segment's own keyframe: ffmpeg
        // starts at the keyframe at or before -ss, so -ss sits just past it,
        // plus the 3/23 s ffmpeg seeks early for video with B-frames. Its
        // cuts are the playlist's later keyframes.
        #[tokio::test]
        async fn a_copied_stream_restarts_on_the_segments_keyframe() {
            for (b_frames, ss) in [(false, "-ss 22.002"), (true, "-ss 22.132")] {
                let root = root(if b_frames { "copy-b" } else { "copy" });
                let tm = manager(&root, fake("runs"));
                tm.start_transcode(request("s4", copied(b_frames), 0))
                    .await
                    .unwrap();
                tm.restart_at_segment("s4", 3).await.unwrap();

                let restart = calls(&root, 2).await[1].clone();
                assert!(restart.contains(ss), "{restart}");
                assert!(restart.contains("-segment_start_number 3"), "{restart}");
                assert!(
                    restart.contains("-segment_times 30.9995,37.9995"),
                    "{restart}"
                );
                tm.kill_all().await;
            }
        }

        // ffmpeg reports file time (-copyts). Resumed at 41:06 and 65 s into
        // the film from there: a segment more than two minutes past that
        // restarts the encoder, one before the run's start does too.
        #[tokio::test]
        async fn restart_decisions_compare_file_times() {
            let root = root("clock");
            let tm = manager(&root, fake("runs"));
            tm.start_transcode(request("s5", encoded(), 411))
                .await
                .unwrap();
            tm.update_position("s5", (2466 + 65) * SEC).await;

            assert!(
                !tm.needs_restart("s5", 441).await,
                "2646 s is within 2 min of 2531 s"
            );
            assert!(tm.needs_restart("s5", 442).await, "2652 s is beyond it");
            assert!(
                tm.needs_restart("s5", 410).await,
                "before the run's start nothing will write it"
            );
            assert!(
                tm.wait_for_segment("s5", 411, Duration::from_secs(3))
                    .await
                    .is_some(),
                "the run's first segment"
            );
            assert!(
                !tm.needs_restart("s5", 411).await,
                "a written segment never"
            );
            tm.kill_all().await;
        }

        // A restart writes new files, so segments the old run finished stay
        // whole and served.
        #[tokio::test]
        async fn segments_of_an_earlier_run_are_still_served() {
            let root = root("runs");
            let tm = manager(&root, fake("runs"));
            tm.start_transcode(request("s6", encoded(), 0))
                .await
                .unwrap();
            let first = tm
                .wait_for_segment("s6", 0, Duration::from_secs(3))
                .await
                .unwrap();
            tm.restart_at_segment("s6", 20).await.unwrap();
            let later = tm
                .wait_for_segment("s6", 20, Duration::from_secs(3))
                .await
                .unwrap();
            assert_ne!(first.file_name(), later.file_name());
            assert_eq!(
                tm.wait_for_segment("s6", 0, Duration::from_secs(1)).await,
                Some(first)
            );
            tm.kill_all().await;
        }

        // A run that finished without writing a segment (past the video's end)
        // answers at once rather than after the full wait.
        #[tokio::test]
        async fn a_segment_a_finished_run_never_wrote_is_not_waited_for() {
            let root = root("past-end");
            let tm = manager(&root, fake("finishes"));
            tm.start_transcode(request("s7", encoded(), 0))
                .await
                .unwrap();
            wait_exit(&tm, "s7").await;
            let started = std::time::Instant::now();
            assert!(
                tm.wait_for_segment("s7", 5, Duration::from_secs(10))
                    .await
                    .is_none()
            );
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "{:?}",
                started.elapsed()
            );
        }

        // ffmpeg runs with -nostdin, so the 'q' written to its stdin never
        // reached it: every stop waited 5 s and force-killed it, holding the
        // manager's lock (and so every other transcode request) meanwhile.
        #[tokio::test]
        async fn stopping_a_running_transcode_is_prompt() {
            let root = root("stop");
            let tm = manager(&root, fake("deaf"));
            tm.start_transcode(request("s8", encoded(), 0))
                .await
                .unwrap();
            tm.start_transcode(request("s9", encoded(), 0))
                .await
                .unwrap();
            tm.jobs.read().await["s9"].pause(); // throttled

            let started = std::time::Instant::now();
            tm.kill_job("s8").await;
            tm.kill_job("s9").await;
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "took {:?}",
                started.elapsed()
            );
            assert!(tm.get_job_output_dir("s8").await.is_none());
        }

        #[test]
        fn a_runs_list_names_its_finished_segments() {
            let hls = "#EXTM3U\n#EXTINF:6,\nr2_40.ts\n#EXTINF:5,\nr2_41.ts\n";
            assert_eq!(listed_segments(hls, 2, "ts"), vec![40, 41]);
            let csv = "r3_7.ts,1.000000,2.000000\nr3_8.ts,2.000000,3.000000\n";
            assert_eq!(listed_segments(csv, 3, "ts"), vec![7, 8]);
            let mixed = "r1_9.ts\nr3_10.m4s\nr3_init.mp4\nr3_11.ts\nr13_12.ts\n";
            assert_eq!(
                listed_segments(mixed, 3, "ts"),
                vec![11],
                "this run's, this kind"
            );
        }

        // Throttling pauses an encoder that is far ahead of the player, measured
        // from the segment the player last asked for, both in file time.
        #[tokio::test]
        async fn throttling_measures_from_the_player() {
            let root = root("throttle");
            let tm = manager(&root, fake("runs"));
            tm.start_transcode(request("s10", encoded(), 411))
                .await
                .unwrap();
            tm.touch_segment("s10", 431).await; // the player is at 2586 s
            tm.update_position("s10", (2586 + 30) * SEC).await;

            tm.throttle_jobs().await;
            assert!(
                !tm.jobs.read().await["s10"].is_paused,
                "30 s ahead is not far ahead"
            );

            tm.update_position("s10", (2586 + 130) * SEC).await;
            tm.throttle_jobs().await;
            assert!(tm.jobs.read().await["s10"].is_paused, "130 s ahead is");

            // The player asks for a segment the paused encoder has not written:
            // the request must resume it, or the segment never comes.
            tm.touch_segment("s10", 450).await;
            assert!(
                !tm.jobs.read().await["s10"].is_paused,
                "a segment request resumes a paused encoder"
            );
            tm.kill_all().await;
        }
    }

    #[test]
    fn test_parse_ffmpeg_time_basic() {
        let ticks = parse_ffmpeg_time("00:01:30.00").unwrap();
        assert_eq!(ticks, 900_000_000); // 90 seconds = 900M ticks
    }

    #[test]
    fn test_parse_ffmpeg_time_with_ms() {
        let ticks = parse_ffmpeg_time("00:00:05.50").unwrap();
        assert_eq!(ticks, 55_000_000); // 5.5 seconds
    }

    #[test]
    fn test_parse_ffmpeg_time_hours() {
        let ticks = parse_ffmpeg_time("02:00:00.00").unwrap();
        assert_eq!(ticks, 72_000_000_000); // 2 hours
    }

    #[test]
    fn test_parse_ffmpeg_time_invalid() {
        assert!(parse_ffmpeg_time("invalid").is_none());
        assert!(parse_ffmpeg_time("00:00").is_none());
    }
}
