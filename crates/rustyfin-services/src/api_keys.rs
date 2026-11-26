//! API keys for programs (library notifications, scripts). A key is shown
//! once, when made; only its hash is kept, and a key is found by its hash.

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

/// API key rules over a data layer.
pub struct ApiKeys<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> ApiKeys<'_, D> {
    /// Make a key named `name`; returns it and the key itself, which is not
    /// kept and cannot be shown again.
    pub async fn create(&self, name: &str) -> ServiceResult<(ApiKey, String)> {
        let key = generate();
        let made = self.data.api_keys().create(name, &hash(&key)).await?;
        Ok((made, key))
    }

    /// Every key (without the keys themselves).
    pub async fn list(&self) -> ServiceResult<Vec<ApiKey>> {
        Ok(self.data.api_keys().list_all().await?)
    }

    /// Delete a key.
    pub async fn delete(&self, id: Uuid) -> ServiceResult<()> {
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

    #[test]
    fn generated_keys_have_the_prefix_and_length() {
        let key = generate();
        assert!(key.starts_with("rfy_"));
        assert_eq!(key.len(), 36);
        assert!(key[4..].chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(generate(), key);
    }
}
