use std::num::NonZeroU32;

use governor::{
    Quota, RateLimiter as GovRateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, direct::NotKeyed},
};

type Inner = GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// A simple rate limiter wrapping `governor`. Call `wait()` before each API request.
pub struct RateLimiter {
    inner: Inner,
}

impl RateLimiter {
    /// Create a rate limiter that allows `requests_per_window` requests per `window_secs` seconds.
    /// Zero values are clamped to safe defaults (1 request per 1 second).
    pub fn new(requests_per_window: u32, window_secs: u64) -> Self {
        let window = window_secs.max(1);
        let burst = requests_per_window.max(1);
        let quota = Quota::with_period(std::time::Duration::from_secs(window))
            .expect("non-zero duration")
            .allow_burst(NonZeroU32::new(burst).expect("non-zero burst"));
        Self {
            inner: GovRateLimiter::direct(quota),
        }
    }

    /// Wait until a request is allowed. Returns immediately if under the limit.
    pub async fn wait(&self) {
        self.inner.until_ready().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "5 requests per 1 s window" allows 10 requests in about a second, not one
    /// request per window after the first five (which took 5 s, and made
    /// `rustyfin artwork` crawl at one TMDB call every 10 s).
    #[tokio::test]
    async fn requests_per_window_means_that_many_per_window() {
        let limiter = RateLimiter::new(5, 1);
        let started = std::time::Instant::now();
        for _ in 0..10 {
            limiter.wait().await;
        }
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_millis(2500), "{took:?}");
        assert!(
            took >= std::time::Duration::from_millis(800),
            "still limited: {took:?}"
        );
    }
}
