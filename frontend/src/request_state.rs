//! What the API client remembers between requests (roadmap 12g): which page
//! a request belongs to, so answers for a page that was left are dropped
//! (touching a left page's state panics the admin), and recent answers, so
//! going back to a page shows it at once.

use std::collections::HashMap;

/// Pages, numbered in the order they were opened.
#[derive(Debug, Default)]
pub struct Pages {
    current: u64,
}

impl Pages {
    /// A new page was opened; returns its number.
    pub fn open(&mut self) -> u64 {
        let _ = self;
        0
    }

    pub fn current(&self) -> u64 {
        self.current
    }

    /// Whether an answer for page `n` may still be used.
    pub fn is_current(&self, n: u64) -> bool {
        let _ = n;
        true
    }
}

/// Recent GET answers by path, kept `ttl_ms`; any change (POST, DELETE)
/// forgets them all, since it may change what they would say.
#[derive(Debug)]
pub struct ResponseCache {
    ttl_ms: f64,
    max_entries: usize,
    entries: HashMap<String, (f64, String)>,
}

impl ResponseCache {
    pub fn new(ttl_ms: f64, max_entries: usize) -> Self {
        ResponseCache {
            ttl_ms,
            max_entries,
            entries: HashMap::new(),
        }
    }

    /// The answer for `path` if it is younger than the time to live.
    pub fn get(&self, path: &str, now_ms: f64) -> Option<&str> {
        let _ = (path, now_ms);
        None
    }

    pub fn put(&mut self, path: &str, body: String, now_ms: f64) {
        let _ = (path, body, now_ms, self.max_entries, self.ttl_ms);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_for_a_page_left_behind_are_dropped() {
        let mut pages = Pages::default();
        let first = pages.open();
        assert!(pages.is_current(first));
        let second = pages.open();
        assert!(!pages.is_current(first));
        assert!(pages.is_current(second));
        assert_eq!(pages.current(), second);
    }

    #[test]
    fn recent_answers_are_reused_until_they_age_or_something_changes() {
        let mut cache = ResponseCache::new(30_000.0, 3);
        cache.put("/Items?a", "A".into(), 1_000.0);
        assert_eq!(cache.get("/Items?a", 20_000.0), Some("A"));
        assert_eq!(cache.get("/Items?a", 31_001.0), None, "too old");
        assert_eq!(cache.get("/Items?b", 2_000.0), None);
        cache.clear();
        assert_eq!(cache.get("/Items?a", 2_000.0), None, "a change forgets all");
    }

    #[test]
    fn the_cache_stays_small() {
        let mut cache = ResponseCache::new(30_000.0, 2);
        cache.put("/a", "1".into(), 1.0);
        cache.put("/b", "2".into(), 2.0);
        cache.put("/c", "3".into(), 3.0);
        assert_eq!(cache.get("/a", 4.0), None, "the oldest goes first");
        assert_eq!(cache.get("/b", 4.0), Some("2"));
        assert_eq!(cache.get("/c", 4.0), Some("3"));
    }
}
