use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use tokio::process::Child;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use super::manager::Plan;

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
    pub kill_timer: Option<JoinHandle<()>>,
    /// Whether the ffmpeg process is currently paused (SIGSTOP'd) for throttling.
    pub is_paused: bool,
    /// The session's segments and ffmpeg settings (absent only for jobs
    /// planted in tests).
    pub plan: Option<Plan>,
    /// The current ffmpeg run (its files are `r<run>_<n>.<ext>`) and the
    /// segment it started at.
    pub run: u32,
    pub run_start: u32,
    /// Segments some run finished, and their files.
    pub done: BTreeMap<u32, PathBuf>,
    /// How far the current run has got, in file time (ffmpeg runs with
    /// `-copyts`).
    pub encoded_secs: f64,
    /// Where the segment the player last asked for starts, in file time.
    pub player_secs: f64,
}

impl TranscodeJob {
    /// Stop ffmpeg and remove the session's output.
    pub async fn kill(&mut self) {
        self.stop_process().await;

        // Cancel any pending kill timer
        if let Some(timer) = self.kill_timer.take() {
            timer.abort();
        }

        self.cleanup().await;
    }

    /// Stop the ffmpeg process: SIGTERM (ffmpeg then ends its output cleanly)
    /// after a SIGCONT in case throttling paused it, wait up to 5 s, then
    /// force kill. ffmpeg runs with `-nostdin`, so a `q` on stdin is never read.
    pub async fn stop_process(&mut self) {
        if let Some(process) = self.process.as_mut() {
            stop_child(&self.play_session_id, process).await;
        }
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

/// Stop an ffmpeg process: SIGCONT (throttling may have paused it), SIGTERM
/// (ffmpeg then ends its output cleanly), up to 5 s, then a kill.
pub async fn stop_child(play_session_id: &str, process: &mut Child) {
    if let Ok(Some(_)) = process.try_wait() {
        return; // already exited
    }
    #[cfg(unix)]
    if let Some(pid) = process.id() {
        // SAFETY: plain signal sends to the child we spawned.
        unsafe {
            libc::kill(pid as i32, libc::SIGCONT);
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    match tokio::time::timeout(std::time::Duration::from_secs(5), process.wait()).await {
        Ok(Ok(status)) => {
            info!(play_session_id, "ffmpeg stopped: {status}");
        }
        Ok(Err(e)) => {
            warn!(play_session_id, "ffmpeg wait error: {e}");
        }
        Err(_) => {
            warn!(play_session_id, "ffmpeg did not exit in 5s, force killing");
            let _ = process.kill().await;
        }
    }
}
