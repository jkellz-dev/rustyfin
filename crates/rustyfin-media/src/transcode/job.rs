use std::path::PathBuf;

use chrono::{DateTime, Utc};
use tokio::process::Child;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use super::params::StreamParams;

pub struct TranscodeJob {
    pub id: Uuid,
    pub play_session_id: String,
    pub item_id: Uuid,
    pub device_id: String,
    pub user_id: Uuid,
    pub output_dir: PathBuf,
    pub process: Option<Child>,
    pub started_at: DateTime<Utc>,
    pub last_segment_request: DateTime<Utc>,
    /// How far ffmpeg has encoded, on the session clock (100ns ticks from the
    /// start of segment 0).
    pub transcoding_position_ticks: i64,
    /// Where this encoder's first segment sits on the session clock (its
    /// `-start_number` times the segment duration); ffmpeg's own clock starts
    /// at 0 there.
    pub offset_ticks: i64,
    /// The file position of segment 0, so a restart at segment n seeks to
    /// this plus n segments.
    pub session_start_ticks: i64,
    /// The start of the segment the player last asked for, on the session clock.
    pub player_ticks: i64,
    pub kill_timer: Option<JoinHandle<()>>,
    /// The segment duration in seconds, needed for seek calculations.
    pub segment_duration: u32,
    /// Cached stream params for seek/restart support.
    pub stream_params_cache: Option<StreamParams>,
    /// Whether the ffmpeg process is currently paused (SIGSTOP'd) for throttling.
    pub is_paused: bool,
}

impl TranscodeJob {
    /// Gracefully stop the ffmpeg process: send 'q' to stdin, wait up to 5s, then force kill.
    pub async fn kill(&mut self) {
        if let Some(ref mut process) = self.process {
            // Try graceful shutdown via stdin 'q'
            if let Some(ref mut stdin) = process.stdin {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(b"q").await;
            }

            // Wait up to 5 seconds for graceful exit
            match tokio::time::timeout(std::time::Duration::from_secs(5), process.wait()).await {
                Ok(Ok(status)) => {
                    info!(
                        play_session_id = %self.play_session_id,
                        "ffmpeg exited gracefully: {status}"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        play_session_id = %self.play_session_id,
                        "ffmpeg wait error: {e}"
                    );
                }
                Err(_) => {
                    warn!(
                        play_session_id = %self.play_session_id,
                        "ffmpeg did not exit in 5s, force killing"
                    );
                    let _ = process.kill().await;
                }
            }
        }

        // Cancel any pending kill timer
        if let Some(timer) = self.kill_timer.take() {
            timer.abort();
        }

        self.cleanup().await;
    }

    /// Whether the session can still serve the player: ffmpeg is running, or it
    /// finished the file (exit 0) and its segments are all on disk. A failed or
    /// killed encoder is not.
    pub fn is_usable(&mut self) -> bool {
        match self.process.as_mut().map(|p| p.try_wait()) {
            Some(Ok(None)) => true,
            Some(Ok(Some(status))) => status.success(),
            _ => false,
        }
    }

    /// Check if the ffmpeg process is still running.
    pub fn is_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            matches!(process.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// Pause the ffmpeg process (SIGSTOP on Unix).
    #[cfg(unix)]
    pub fn pause(&self) {
        if let Some(ref process) = self.process
            && let Some(pid) = process.id()
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
            info!(
                play_session_id = %self.play_session_id,
                "paused ffmpeg (SIGSTOP)"
            );
        }
    }

    /// Resume the ffmpeg process (SIGCONT on Unix).
    #[cfg(unix)]
    pub fn resume(&self) {
        if let Some(ref process) = self.process
            && let Some(pid) = process.id()
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
            info!(
                play_session_id = %self.play_session_id,
                "resumed ffmpeg (SIGCONT)"
            );
        }
    }

    /// No-op on non-Unix platforms.
    #[cfg(not(unix))]
    pub fn pause(&self) {}

    /// No-op on non-Unix platforms.
    #[cfg(not(unix))]
    pub fn resume(&self) {}

    /// Remove the output directory and all segment files.
    pub async fn cleanup(&self) {
        if self.output_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&self.output_dir).await {
                warn!(
                    play_session_id = %self.play_session_id,
                    path = %self.output_dir.display(),
                    "failed to clean up transcode output: {e}"
                );
            } else {
                info!(
                    play_session_id = %self.play_session_id,
                    "cleaned up transcode output at {}",
                    self.output_dir.display()
                );
            }
        }
    }
}
