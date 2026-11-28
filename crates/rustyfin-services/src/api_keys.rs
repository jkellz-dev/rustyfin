//! API keys for programs (library notifications, scripts, media managers'
//! webhooks). A key is shown once, when made; only its hash is kept, and a
//! key is found by its hash.
//!
//! Owner, 2026-11-27: keys come from `rustyfin.toml` (`[[api_key]]`, a name
//! and a `key_file`, "So secrets can be used"). Once the file sets any, only
//! its keys work, and keys are no longer made or deleted in the admin
//! (409), as with libraries. A file that sets none keeps the keys made in
//! the admin.

use rand::RngExt;
use rustyfin_models::data::{ApiKey, ApiKeyStore, Data};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

/// The hash a key is stored and looked up by: lowercase hex SHA-256.
pub fn hash(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// A new random key: `rfy_` and 32 letters and digits.
pub fn generate() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let random_part: String = (0..32)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect();
    format!("rfy_{random_part}")
}

/// A key `rustyfin.toml` sets: its name and the key's hash.
#[derive(Debug, Clone)]
pub struct ConfiguredKey {
    pub name: String,
    hash: String,
    /// When this server read it (what `GET /Auth/Keys` shows as made).
    read_at: chrono::DateTime<chrono::Utc>,
}

impl ConfiguredKey {
    pub fn new(name: String, key: &str) -> Self {
        Self {
            name,
            hash: hash(key.trim()),
            read_at: chrono::Utc::now(),
        }
    }

    /// A stable id, so the admin's list keeps it across restarts.
    fn id(&self) -> Uuid {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("api-key:{}", self.name).as_bytes(),
        )
    }
}

/// Who a key belongs to.
#[derive(Debug, Clone)]
pub struct KeyHolder {
    pub name: String,
}

const SET_IN_FILE: &str = "API keys are set in rustyfin.toml ([[api_key]])";

/// API key rules over a data layer.
pub struct ApiKeys<'a, D> {
    pub(crate) data: &'a D,
    /// The keys `rustyfin.toml` sets; `None` when it sets none.
    pub(crate) configured: Option<&'a [ConfiguredKey]>,
}

impl<D: Data> ApiKeys<'_, D> {
    /// Whose key `key` is, if it is one: a key the file sets, or, when the
    /// file sets none, one made in the admin (its last use is recorded).
    pub async fn find(&self, key: &str) -> ServiceResult<Option<KeyHolder>> {
        let wanted = hash(key.trim());
        if let Some(configured) = self.configured {
            return Ok(configured
                .iter()
                .find(|k| k.hash == wanted)
                .map(|k| KeyHolder {
                    name: k.name.clone(),
                }));
        }
        let Some(row) = self.data.api_keys().find_by_hash(&wanted).await? else {
            return Ok(None);
        };
        let data = self.data.clone();
        tokio::spawn(async move {
            let _ = data.api_keys().touch_last_used(row.id).await;
        });
        Ok(Some(KeyHolder { name: row.name }))
    }

    /// Make a key named `name`; returns it and the key itself, which is not
    /// kept and cannot be shown again. Refused once the file sets keys.
    pub async fn create(&self, name: &str) -> ServiceResult<(ApiKey, String)> {
        if self.configured.is_some() {
            return Err(ServiceError::Conflict(SET_IN_FILE.to_string()));
        }
        let key = generate();
        let made = self.data.api_keys().create(name, &hash(&key)).await?;
        Ok((made, key))
    }

    /// Every key (without the keys themselves): the file's, when it sets
    /// any.
    pub async fn list(&self) -> ServiceResult<Vec<ApiKey>> {
        if let Some(configured) = self.configured {
            return Ok(configured
                .iter()
                .map(|k| ApiKey {
                    id: k.id(),
                    name: k.name.clone(),
                    key_hash: String::new(),
                    created_at: k.read_at,
                    last_used_at: None,
                })
                .collect());
        }
        Ok(self.data.api_keys().list_all().await?)
    }

    /// Delete a key. Refused once the file sets keys.
    pub async fn delete(&self, id: Uuid) -> ServiceResult<()> {
        if self.configured.is_some() {
            return Err(ServiceError::Conflict(SET_IN_FILE.to_string()));
        }
        if self.data.api_keys().delete(id).await? {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stored API keys are looked up by this text: it must not change when
    /// the hashing crate does (sha2 0.11 dropped `{:x}` on digests).
    #[test]
    fn stored_key_hashes_keep_their_form() {
        assert_eq!(
            hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// A key file usually ends in a newline; the key is what is before it.
    #[test]
    fn a_configured_key_ignores_the_files_trailing_newline() {
        let key = ConfiguredKey::new("hooks".to_string(), "secret\n");
        assert_eq!(key.hash, hash("secret"));
        assert_eq!(key.id(), ConfiguredKey::new("hooks".to_string(), "x").id());
    }

    #[test]
    fn generated_keys_have_the_prefix_and_length() {
        let key = generate();
        assert!(key.starts_with("rfy_"));
        assert_eq!(key.len(), 36);
        assert!(key[4..].chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(generate(), key);
    }
}
