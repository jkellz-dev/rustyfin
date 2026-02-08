//! Media probing functionality using ffprobe.
//! Extracts codec information, duration, streams, and metadata.

/// Duration in 100-nanosecond ticks (Jellyfin convention).
pub fn seconds_to_ticks(seconds: f64) -> i64 {
    (seconds * 10_000_000.0) as i64
}

/// Convert ticks back to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}
