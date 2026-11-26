//! The HTTP API: every Jellyfin route Rustyfin answers, plus its own.
//!
//! [`router`] builds the axum router and the shared [`router::AppState`];
//! `routes` holds one module per area (items, playback, HLS, sessions,
//! users, libraries, play history, devices, WebSocket, ...). Around them:
//! [`error`] (one `ApiError` mapped to status codes and log levels),
//! [`query_case`] (Jellyfin's query names in any case), [`request_log`] and
//! [`metrics`] (what clients ask for, Prometheus), [`health`] and [`cors`].

pub mod api_v1;
pub mod cors;
pub mod error;
pub mod guid_format;
pub mod health;
pub mod identity;
pub mod link_notices;
pub mod link_scope;
pub mod metrics;
pub mod play_method;
pub mod playback_info;
pub mod query_case;
pub mod remote;
pub mod request_log;
pub mod route_case;
pub mod router;
pub mod routes;
pub mod telemetry;
pub mod user_prefs;
