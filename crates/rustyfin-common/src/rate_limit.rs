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
