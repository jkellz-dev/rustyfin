//! What the routes share.

use std::sync::Arc;

use rustyfin_services::proxy::{ForwardAuthHeaders, TrustedProxies};

/// The trusted proxy in front (`[auth.forward_auth]`): who it is, the
/// headers it names the user in, and the group whose members administer.
#[derive(Debug, Clone)]
pub struct Proxy {
    pub trusted: TrustedProxies,
    pub headers: ForwardAuthHeaders,
    pub admin_group: Option<String>,
}

/// The API's state.
#[derive(Clone)]
pub struct ApiState<D> {
    /// The data layer (`CLAUDE.md`, "Layers").
    pub data: D,
    /// The rules over `data`.
    pub services: rustyfin_services::Services<D>,
    /// Linked servers (roadmap 16b).
    pub links: Arc<rustyfin_services::link::Links>,
    /// The proxy that names who is asking; `None` in development, where a
    /// session token alone does.
    pub proxy: Option<Proxy>,
}
