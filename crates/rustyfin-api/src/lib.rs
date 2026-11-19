//! The HTTP API: every Jellyfin route Rustyfin answers, plus its own.
//!
//! [`router`] builds the axum router and the shared [`router::AppState`];
//! `routes` holds one module per area (items, playback, HLS, sessions,
//! users, libraries, play history, devices, WebSocket, ...). Around them:
//! [`error`] (one `ApiError` mapped to status codes and log levels),
//! [`query_case`] (Jellyfin's query names in any case), [`play_state`]
//! (Jellyfin 10.10's played and resume rules), [`request_log`] and
//! [`metrics`] (what clients ask for, Prometheus), [`health`],
//! [`login_limit`] and [`cors`].

pub mod api_v1;
pub mod cors;
pub mod error;
pub mod health;
pub mod link_notices;
pub mod login_limit;
pub mod metrics;
pub mod play_state;
pub mod query_case;
pub mod request_log;
pub mod router;
pub mod routes;
