//! Linked servers (roadmap 16b, owner 2026-11-18).
//!
//! A friend's Jellyfin (or Rustyfin) shares some of its libraries with this
//! server through a user made for the link. This server signs in as that
//! user, keeps a copy of the shared catalogue (never the media), and merges
//! it into its own libraries: a film it already holds gains the friend's copy
//! as another version, the best of which plays by default; a film it lacks is
//! listed like its own. Playback and artwork come through this server, so
//! clients only ever talk to it.
//!
//! The link is configured in the TOML file (`[[link]]`); see
//! `docs/linked-servers.md`.

mod jellyfin;
mod sync;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use jellyfin::JellyfinClient;
pub use rustyfin_db::repo::links::{LINK_SCHEME, parse_image_path};
pub use sync::items_from_jellyfin;

/// What the friend runs. A Rustyfin speaks the Jellyfin API too; its own,
/// faster sync is roadmap 16b.2r.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Jellyfin,
    Rustyfin,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::Jellyfin => "jellyfin",
            LinkKind::Rustyfin => "rustyfin",
        }
    }
}

/// One `[[link]]` in the config file.
#[derive(Clone, Deserialize)]
pub struct LinkSettings {
    /// Shown to users beside the friend's copies ("2160p · Alex").
    pub name: String,
    pub kind: LinkKind,
    /// The friend's server, e.g. `https://jellyfin.example.ts.net`.
    pub url: String,
    /// The user the friend made for the link. It should see only the
    /// libraries they share; an administrator's key is never needed.
    pub user: String,
    pub password: String,
    /// Which of the friend's libraries go into which local library.
    #[serde(default)]
    pub libraries: Vec<LibraryMap>,
}

impl std::fmt::Debug for LinkSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkSettings")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("url", &self.url)
            .field("user", &self.user)
            .field("password", &"<hidden>")
            .field("libraries", &self.libraries)
            .finish()
    }
}

/// The friend's library `remote` (by name) merges into the local `local`.
/// With no `local`, it gets a library of its own, named
/// "<remote> · <link name>", with no folders.
#[derive(Debug, Clone, Deserialize)]
pub struct LibraryMap {
    pub remote: String,
    #[serde(default)]
    pub local: Option<String>,
}

/// What a sync changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub added: u64,
    pub updated: u64,
    pub removed: u64,
    /// Copies that became versions of a title held elsewhere.
    pub merged: u64,
    /// Mappings that could not sync (the rest did). While any fails, nothing
    /// is removed.
    pub errors: Vec<String>,
}

/// One configured link.
pub struct Link {
    pub id: Uuid,
    pub settings: LinkSettings,
    pub client: JellyfinClient,
    /// One sync at a time per link.
    syncing: tokio::sync::Mutex<()>,
}

/// The configured links, by id.
#[derive(Default)]
pub struct Links {
    links: RwLock<HashMap<Uuid, Arc<Link>>>,
}

impl Links {
    /// Record a link (by name) and keep a client for it.
    pub async fn register(&self, pool: &PgPool, settings: LinkSettings) -> anyhow::Result<Uuid> {
        let url = settings.url.trim_end_matches('/').to_string();
        let id =
            rustyfin_db::repo::links::upsert(pool, &settings.name, settings.kind.as_str(), &url)
                .await?;
        let client = JellyfinClient::new(&url, &settings.user, &settings.password, id)?;
        let link = Arc::new(Link {
            id,
            settings,
            client,
            syncing: Default::default(),
        });
        self.links.write().await.insert(id, link);
        Ok(id)
    }

    pub async fn get(&self, id: Uuid) -> Option<Arc<Link>> {
        self.links.read().await.get(&id).cloned()
    }

    pub async fn ids(&self) -> Vec<Uuid> {
        self.links.read().await.keys().copied().collect()
    }

    /// Bring the link's copies up to date, and record how it went.
    pub async fn sync(&self, pool: &PgPool, id: Uuid) -> anyhow::Result<SyncReport> {
        let link = self
            .get(id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no link {id}"))?;
        let _one = link.syncing.lock().await;
        let started = std::time::Instant::now();
        let result = sync::sync_link(pool, &link).await;
        let error = match &result {
            Err(e) => Some(format!("{e:#}")),
            Ok(r) if !r.errors.is_empty() => Some(r.errors.join("; ")),
            Ok(_) => None,
        };
        if let Err(e) = rustyfin_db::repo::links::record_sync(pool, id, error.as_deref()).await {
            tracing::warn!(
                "Could not record the sync of link {}: {e}",
                link.settings.name
            );
        }
        match &result {
            Ok(r) => {
                tracing::info!(
                    "Link {}: {} added, {} updated, {} removed, {} merged in {:?}",
                    link.settings.name,
                    r.added,
                    r.updated,
                    r.removed,
                    r.merged,
                    started.elapsed()
                );
                for e in &r.errors {
                    tracing::warn!("Link {}: {e}", link.settings.name);
                }
            }
            Err(e) => tracing::warn!("Link {} did not sync: {e:#}", link.settings.name),
        }
        result
    }

    /// Sync every link; failures are logged and recorded, not returned.
    pub async fn sync_all(&self, pool: &PgPool) {
        for id in self.ids().await {
            let _ = self.sync(pool, id).await;
        }
    }
}
