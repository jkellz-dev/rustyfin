use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// A simple disk-backed JSON cache with TTL based on file mtime.
pub struct DiskCache {
    base_dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf, ttl_days: u32) -> Self {
        Self {
            base_dir,
            ttl: Duration::from_secs(u64::from(ttl_days) * 86400),
        }
    }

    /// Try to read a cached value. Returns `None` if missing or expired.
    pub fn get<T: DeserializeOwned>(&self, namespace: &str, key: &str) -> Option<T> {
        let path = self.path_for(namespace, key);
        let metadata = std::fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        if modified.elapsed().ok()? > self.ttl {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Write a value to the cache.
    pub fn put<T: Serialize>(&self, namespace: &str, key: &str, value: &T) {
        let path = self.path_for(namespace, key);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("tmp");
        if let Ok(data) = serde_json::to_string(value)
            && std::fs::write(&tmp, &data).is_ok()
        {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// Hash a query string into a filesystem-safe key.
    pub fn hash_key(query: &str) -> String {
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn path_for(&self, namespace: &str, key: &str) -> PathBuf {
        self.base_dir.join(namespace).join(format!("{key}.json"))
    }
}
