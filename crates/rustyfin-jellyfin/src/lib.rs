//! The HTTP API: every Jellyfin route Rustyfin answers, plus its own.
//!
//! [`router`] builds the axum router and the shared [`router::AppState`];
//! `routes` holds one module per area (items, playback, HLS, sessions,
//! users, libraries, play history, devices, WebSocket, ...). Around them:
//! [`error`] (one `ApiError` mapped to status codes and log levels),
//! [`query_case`] (Jellyfin's query names in any case), [`health`] (the
//! dashboard's health panel). Metrics, the request log, CORS and tracing
//! are the server's (`rustyfin-server`), applied to every route.

pub mod api_v1;
pub mod auth;
pub mod dto;
pub mod enums;
pub mod error;
pub mod guid_format;
pub mod health;
pub mod identity;
pub mod link_notices;
pub mod link_scope;
pub mod play_method;
pub mod playback_info;
pub mod query_case;
pub mod remote;
pub mod route_case;
pub mod router;
pub mod routes;
pub mod user_prefs;
