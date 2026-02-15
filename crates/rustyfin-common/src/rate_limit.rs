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
    pub fn new(requests_per_window: u32, window_secs: u64) -> Self {
        let quota = Quota::with_period(std::time::Duration::from_secs(window_secs))
            .expect("valid duration")
            .allow_burst(NonZeroU32::new(requests_per_window).expect("non-zero rate limit"));
        Self {
            inner: GovRateLimiter::direct(quota),
        }
    }

    /// Wait until a request is allowed. Returns immediately if under the limit.
    pub async fn wait(&self) {
        self.inner.until_ready().await;
    }
}
