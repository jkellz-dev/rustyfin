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
pub use jellyfin::LinkStats;
mod oidc;
mod sync;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use jellyfin::{Changed, Feed, JellyfinClient};
pub use oidc::OidcSettings;
pub use rustyfin_data::repo::links::{LINK_SCHEME, parse_image_path};
pub use sync::items_from_jellyfin;

/// What the friend runs. A Rustyfin is signed in to through its forward-auth
/// proxy, with no password, and synced from its change feed (roadmap 16b.2r).
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
    /// libraries they share; an administrator's key is never needed. Empty for
    /// a Rustyfin: its proxy says who this server is.
    pub user: String,
    pub password: String,
    /// Which of the friend's libraries go into which local library.
    #[serde(default)]
    pub libraries: Vec<LibraryMap>,
    /// A client-credentials token to fetch for the friend's proxy, when it
    /// takes OIDC tokens (`[link.oidc]`).
    #[serde(default)]
    pub oidc: Option<OidcSettings>,
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
            .field("oidc", &self.oidc)
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

/// A link's server went down or came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEvent {
    pub link_id: Uuid,
    pub name: String,
    pub online: bool,
}

/// The configured links, by id.
pub struct Links {
    links: RwLock<HashMap<Uuid, Arc<Link>>>,
    events: tokio::sync::broadcast::Sender<LinkEvent>,
}

impl Default for Links {
    fn default() -> Self {
        Self {
            links: RwLock::default(),
            events: tokio::sync::broadcast::channel(16).0,
        }
    }
}

impl Links {
    /// Record a link (by name) and keep a client for it.
    pub async fn register(&self, pool: &PgPool, settings: LinkSettings) -> anyhow::Result<Uuid> {
        let url = settings.url.trim_end_matches('/').to_string();
        let id =
            rustyfin_data::repo::links::upsert(pool, &settings.name, settings.kind.as_str(), &url)
                .await?;
        let client = JellyfinClient::new(&url, &settings.user, &settings.password, id)?
            .with_oidc(settings.oidc.clone())
            .named(&settings.name);
        let link = Arc::new(Link {
            id,
            settings,
            client,
            syncing: Default::default(),
        });
        self.links.write().await.insert(id, link);
        Ok(id)
    }

    /// Links going down and coming back, for telling users.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LinkEvent> {
        self.events.subscribe()
    }

    /// Record that a link's server is up or down. On a change: titles it
    /// stood for move to a copy that is up (going down) or the shows are
    /// placed again (coming back), the activity log says so, and
    /// subscribers hear of it.
    async fn mark(&self, pool: &PgPool, link: &Link, online: bool) {
        let changed = match rustyfin_data::repo::links::set_offline(pool, link.id, !online).await {
            Ok(changed) => changed,
            Err(e) => {
                tracing::warn!(
                    "Link {}: could not record its state: {e}",
                    link.settings.name
                );
                return;
            }
        };
        if !changed {
            return;
        }
        let name = &link.settings.name;
        let moved = if online {
            rustyfin_data::repo::links::merge(pool).await
        } else {
            rustyfin_data::repo::links::rehome(pool, link.id).await
        };
        if let Err(e) = moved {
            tracing::warn!("Link {name}: could not rearrange its copies: {e}");
        }
        let (text, severity) = if online {
            (format!("Linked server {name} is back"), "Information")
        } else {
            (
                format!("Linked server {name} is down; its titles are hidden"),
                "Warning",
            )
        };
        if online {
            tracing::info!("{text}");
        } else {
            tracing::warn!("{text}");
        }
        let _ = rustyfin_data::repo::activity_log::insert(
            pool,
            &text,
            None,
            "LinkedServerStatus",
            severity,
            None,
            None,
        )
        .await;
        let _ = self.events.send(LinkEvent {
            link_id: link.id,
            name: name.clone(),
            online,
        });
    }

    /// Check a link that just failed to answer (a stream, say): mark it down
    /// if it does not answer now. Skipped while it syncs.
    pub async fn check(&self, pool: &PgPool, id: Uuid) {
        let Some(link) = self.get(id).await else {
            return;
        };
        let Ok(_one) = link.syncing.try_lock() else {
            return;
        };
        if link.client.libraries().await.is_err() {
            self.mark(pool, &link, false).await;
        }
    }

    /// Sync each link whose server is down, which brings back those that
    /// answer again. Returns how many came back.
    pub async fn recheck_offline(&self, pool: &PgPool) -> usize {
        let Ok(offline) = rustyfin_data::repo::links::offline_ids(pool).await else {
            return 0;
        };
        let mut back = 0;
        for id in offline {
            if self.get(id).await.is_some() && self.sync(pool, id).await.is_ok() {
                back += 1;
            }
        }
        back
    }

    pub async fn get(&self, id: Uuid) -> Option<Arc<Link>> {
        self.links.read().await.get(&id).cloned()
    }

    /// The link's numbers since this server started; `None` for a link not
    /// in the config file.
    pub async fn stats(&self, id: Uuid) -> Option<LinkStats> {
        self.get(id).await.map(|link| link.client.stats())
    }

    /// True while the link is syncing.
    pub async fn is_syncing(&self, id: Uuid) -> bool {
        match self.get(id).await {
            Some(link) => link.syncing.try_lock().is_err(),
            None => false,
        }
    }

    /// The links that fill a library (named `library_name`; `library_link`
    /// when a link made it), and the friend's library each brings: (link id,
    /// link name, the friend's library), from the config, so known before
    /// the first sync.
    pub async fn sources(
        &self,
        library_name: &str,
        library_link: Option<Uuid>,
    ) -> Vec<(Uuid, String, String)> {
        let here = library_name.trim().to_lowercase();
        let links = self.links.read().await;
        let mut out = Vec::new();
        for link in links.values() {
            for map in &link.settings.libraries {
                let remote = map.remote.trim();
                let fills = match &map.local {
                    Some(local) => local.trim().to_lowercase() == here,
                    None => {
                        library_link == Some(link.id)
                            && format!("{remote} · {}", link.settings.name).to_lowercase() == here
                    }
                };
                if fills {
                    out.push((link.id, link.settings.name.clone(), remote.to_string()));
                }
            }
        }
        out.sort_by(|a, b| (&a.1, &a.2).cmp(&(&b.1, &b.2)));
        out
    }

    /// Sync, one after another, each link that fills any of `libraries`
    /// ((name, the link that made it)), each once: a library's scan is
    /// followed by its links' syncs (owner, 2026-11-23).
    pub async fn sync_for(&self, pool: &PgPool, libraries: &[(String, Option<Uuid>)]) {
        let mut ids: Vec<Uuid> = Vec::new();
        for (name, made_by) in libraries {
            for (id, _, _) in self.sources(name, *made_by).await {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        for id in ids {
            let _ = self.sync(pool, id).await;
        }
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
        // Is the friend's server there at all? Its titles leave the views
        // while it is not (owner, 2026-11-19).
        let result = match link.client.libraries().await {
            Err(e) => {
                self.mark(pool, &link, false).await;
                Err(e)
            }
            Ok(_) => {
                self.mark(pool, &link, true).await;
                sync::sync_link(pool, &link).await
            }
        };
        let error = match &result {
            Err(e) => Some(format!("{e:#}")),
            Ok(r) if !r.errors.is_empty() => Some(r.errors.join("; ")),
            Ok(_) => None,
        };
        let name = link.settings.name.clone();
        metrics::counter!(
            "rustyfin_link_syncs_total",
            "link" => name.clone(),
            "result" => if error.is_none() { "ok" } else { "error" }
        )
        .increment(1);
        metrics::histogram!("rustyfin_link_sync_duration_seconds", "link" => name)
            .record(started.elapsed().as_secs_f64());
        link.client.record_sync(started.elapsed(), error.is_none());
        if let Err(e) = rustyfin_data::repo::links::record_sync(pool, id, error.as_deref()).await {
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
