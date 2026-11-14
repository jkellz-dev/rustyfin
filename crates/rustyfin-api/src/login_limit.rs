//! Throttling of password logins (roadmap 11e).
//!
//! A failed login is remembered for a sliding window under the user name it
//! tried (case-insensitive) and the address it came from. Once a name has
//! `per_name` failures, or an address `per_addr`, inside the window, further
//! attempts are refused with how long until the oldest counted failure leaves
//! it. Nothing is ever locked for good: Jellyfin's lockout (disabling the
//! account after a few failures) lets anyone shut a known user out.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Failures for one key, oldest first.
type Failures = VecDeque<Instant>;

/// Most names or addresses tracked at once. When full, expired entries are
/// dropped; if it is still full, a new key is not tracked (the address limit
/// still applies to whoever floods it with names).
const MAX_KEYS: usize = 10_000;

pub struct LoginLimiter {
    per_name: usize,
    per_addr: usize,
    window: Duration,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    by_name: HashMap<String, Failures>,
    by_addr: HashMap<IpAddr, Failures>,
}

impl Default for LoginLimiter {
    /// 5 failures per name or 20 per address in 15 minutes.
    fn default() -> Self {
        Self::new(5, 20, Duration::from_secs(15 * 60))
    }
}

/// The key a user name is counted under: trimmed and lowercased; none if empty.
fn name_key(name: Option<&str>) -> Option<String> {
    name.map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
}

impl LoginLimiter {
    pub fn new(per_name: usize, per_addr: usize, window: Duration) -> Self {
        LoginLimiter {
            per_name: per_name.max(1),
            per_addr: per_addr.max(1),
            window,
            state: Mutex::new(State::default()),
        }
    }

    /// `Err(wait)` when the name or the address is throttled now.
    pub fn check(&self, addr: IpAddr, name: Option<&str>, now: Instant) -> Result<(), Duration> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut wait = Duration::ZERO;
        if let Some(key) = name_key(name)
            && let Some(failures) = state.by_name.get_mut(&key)
        {
            wait = wait.max(self.wait(failures, self.per_name, now));
        }
        if let Some(failures) = state.by_addr.get_mut(&addr) {
            wait = wait.max(self.wait(failures, self.per_addr, now));
        }
        if wait.is_zero() { Ok(()) } else { Err(wait) }
    }

    /// Count a failed attempt.
    pub fn record_failure(&self, addr: IpAddr, name: Option<&str>, now: Instant) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let window = self.window;
        if let Some(key) = name_key(name) {
            push(&mut state.by_name, key, now, window);
        }
        push(&mut state.by_addr, addr, now, window);
    }

    /// A successful login clears its name's failures (not the address's).
    pub fn record_success(&self, name: Option<&str>) {
        if let Some(key) = name_key(name) {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.by_name.remove(&key);
        }
    }

    /// Drops failures older than the window; how long until fewer than `limit`
    /// remain (zero if already fewer).
    fn wait(&self, failures: &mut Failures, limit: usize, now: Instant) -> Duration {
        expire(failures, now, self.window);
        if failures.len() < limit {
            return Duration::ZERO;
        }
        // The attempt that must expire for the count to drop below the limit.
        let pivot = failures[failures.len() - limit];
        (pivot + self.window)
            .saturating_duration_since(now)
            .max(Duration::from_secs(1))
    }
}

fn expire(failures: &mut Failures, now: Instant, window: Duration) {
    while failures
        .front()
        .is_some_and(|t| now.saturating_duration_since(*t) >= window)
    {
        failures.pop_front();
    }
}

fn push<K: Eq + Hash>(map: &mut HashMap<K, Failures>, key: K, now: Instant, window: Duration) {
    if !map.contains_key(&key) && map.len() >= MAX_KEYS {
        map.retain(|_, f| {
            expire(f, now, window);
            !f.is_empty()
        });
        if map.len() >= MAX_KEYS {
            return;
        }
    }
    map.entry(key).or_default().push_back(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, last])
    }

    #[test]
    fn a_name_is_throttled_at_its_limit_and_released_as_failures_age() {
        let limiter = LoginLimiter::new(3, 100, Duration::from_secs(60));
        let t0 = Instant::now();
        for s in 0..3 {
            assert!(limiter.check(ip(1), Some("Ann"), t0).is_ok());
            limiter.record_failure(ip(1), Some("Ann"), t0 + Duration::from_secs(s * 10));
        }
        let now = t0 + Duration::from_secs(25);
        // The first failure (t0) leaves the window at t0+60: 35 s from now.
        assert_eq!(
            limiter.check(ip(2), Some(" ann "), now),
            Err(Duration::from_secs(35))
        );
        assert!(limiter.check(ip(1), Some("bob"), now).is_ok());
        assert!(
            limiter
                .check(ip(1), Some("ann"), t0 + Duration::from_secs(60))
                .is_ok()
        );
    }

    #[test]
    fn an_address_is_throttled_across_names_and_success_clears_only_the_name() {
        let limiter = LoginLimiter::new(2, 3, Duration::from_secs(60));
        let now = Instant::now();
        for name in ["a", "b", "c"] {
            limiter.record_failure(ip(1), Some(name), now);
        }
        assert!(limiter.check(ip(1), Some("d"), now).is_err());
        assert!(limiter.check(ip(2), Some("d"), now).is_ok());
        limiter.record_failure(ip(2), Some("e"), now);
        limiter.record_failure(ip(3), Some("e"), now);
        assert!(limiter.check(ip(4), Some("e"), now).is_err());
        limiter.record_success(Some("E"));
        assert!(limiter.check(ip(4), Some("e"), now).is_ok());
        assert!(
            limiter.check(ip(1), None, now).is_err(),
            "the address count stays"
        );
    }

    #[test]
    fn tracking_is_bounded() {
        let limiter = LoginLimiter::new(1, usize::MAX, Duration::from_secs(60));
        let now = Instant::now();
        for i in 0..(MAX_KEYS + 50) {
            limiter.record_failure(ip(1), Some(&format!("n{i}")), now);
        }
        let state = limiter.state.lock().unwrap();
        assert_eq!(state.by_name.len(), MAX_KEYS);
    }
}
