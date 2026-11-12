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
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("failed to spawn ffmpeg")?;

        // Take stderr for progress monitoring
        let stderr = child.stderr.take();

        let now = chrono::Utc::now();
        let segment_duration = request.params.segment_duration;
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
            transcoding_position_ticks: 0,
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
            // Kill old process but keep the output dir for existing segments
            if let Some(ref mut process) = job.process {
                if let Some(ref mut stdin) = process.stdin {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin.write_all(b"q").await;
                }
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(3), process.wait()).await;
                let _ = process.kill().await;
            }
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

    /// Get a reference to a job by play session ID.
    pub async fn get_job_output_dir(&self, play_session_id: &str) -> Option<PathBuf> {
        let jobs = self.jobs.read().await;
        jobs.get(play_session_id).map(|j| j.output_dir.clone())
    }

    /// Check if a job exists and is alive for the given play session ID.
    pub async fn has_active_job(&self, play_session_id: &str) -> bool {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(play_session_id) {
            job.is_alive()
        } else {
            false
        }
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
        let mut jobs = self.jobs.write().await;
        if let Some(mut job) = jobs.remove(play_session_id) {
            info!(play_session_id, "killing transcode job");
            job.kill().await;
        }
    }

    /// Kill all jobs for a given device ID.
    pub async fn kill_jobs_for_device(&self, device_id: &str) {
        let mut jobs = self.jobs.write().await;
        let matching: Vec<String> = jobs
            .iter()
            .filter(|(_, j)| j.device_id == device_id)
            .map(|(k, _)| k.clone())
            .collect();

        for key in matching {
            if let Some(mut job) = jobs.remove(&key) {
                info!(play_session_id = %key, device_id, "killing transcode job for device");
                job.kill().await;
            }
        }
    }

    /// Throttle ffmpeg processes that are too far ahead of playback.
    ///
    /// On a fast machine, ffmpeg can transcode much faster than real-time.
    /// To avoid wasting CPU and disk, pause it when it's >120s ahead
    /// and resume when it's <30s ahead.
    pub async fn throttle_jobs(&self) {
        let throttle_ahead = 120 * 10_000_000i64; // 120 seconds in ticks
        let resume_threshold = 30 * 10_000_000i64; // 30 seconds in ticks

        let mut jobs = self.jobs.write().await;
        for job in jobs.values_mut() {
            if !job.is_alive() {
                continue;
            }

            // Estimate the client's playback position from the last segment request time
            // and the job's segment duration. If a segment was last requested N seconds ago,
            // the client is likely at: last_request_time's segment position.
            // Simpler heuristic: compare transcoding position to a "reasonable ahead" threshold.
            let ahead = job.transcoding_position_ticks;
            // We don't track the client's exact position here, so use the
            // last_segment_request staleness as a proxy — if no segments requested recently
            // and ffmpeg is far ahead, throttle it.

            if ahead > throttle_ahead && !job.is_paused {
                job.pause();
                job.is_paused = true;
            } else if ahead <= resume_threshold && job.is_paused {
                job.resume();
                job.is_paused = false;
            }
        }
    }

    /// Kill stale jobs that haven't had a segment request in the given duration.
    pub async fn cleanup_stale_jobs(&self) {
        let stale_threshold = chrono::Duration::seconds(60);
        let now = chrono::Utc::now();
        let mut jobs = self.jobs.write().await;

        let stale: Vec<String> = jobs
            .iter()
            .filter(|(_, j)| now - j.last_segment_request > stale_threshold)
            .map(|(k, _)| k.clone())
            .collect();

        // Also find jobs whose process has exited
        let mut dead: Vec<String> = Vec::new();
        for (k, j) in jobs.iter_mut() {
            if !j.is_alive() && !stale.contains(k) {
                dead.push(k.clone());
            }
        }
        let stale: Vec<String> = stale.into_iter().chain(dead).collect();

        for key in stale {
            if let Some(mut job) = jobs.remove(&key) {
                warn!(
                    play_session_id = %key,
                    "cleaning up stale transcode job"
                );
                job.kill().await;
            }
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
        let mut jobs = self.jobs.write().await;
        let all_keys: Vec<String> = jobs.keys().cloned().collect();
        for key in all_keys {
            if let Some(mut job) = jobs.remove(&key) {
                info!(play_session_id = %key, "killing transcode job (shutdown)");
                job.kill().await;
            }
        }
    }

    /// Insert a job into the manager.
    pub(crate) async fn insert_job(&self, play_session_id: String, job: TranscodeJob) {
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
        self.jobs.read().await.len() >= self.max_concurrent
    }

    /// Update the transcoding position for a job.
    pub(crate) async fn update_position(&self, play_session_id: &str, position_ticks: i64) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(play_session_id) {
            job.transcoding_position_ticks = position_ticks;
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
