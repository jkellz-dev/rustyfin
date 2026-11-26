//! The server: what the `rustyfin-server` binary puts together, shared with
//! the tests so they run the same composition.
//!
//! [`app`] mounts the API routers; around every route the server applies
//! what is the same for both: [`metrics`] (counting and timing requests,
//! and the `/metrics` listener), [`request_log`], [`telemetry`] (logs and
//! traces) and [`cors`].

pub mod cors;
pub mod metrics;
pub mod request_log;
pub mod telemetry;

use axum::Router;
use rustyfin_models::data::Data;

/// What the server does to every API route: counts and times it, and logs
/// it at debug level.
pub fn observe(router: Router) -> Router {
    router
        .route_layer(axum::middleware::from_fn(metrics::track_requests))
        .layer(axum::middleware::from_fn(request_log::log_request))
}

/// The API: the Jellyfin routes, each route observed ([`observe`]).
pub fn app<D: Data>(jellyfin: rustyfin_jellyfin::router::AppState<D>) -> Router {
    rustyfin_jellyfin::router::build_router(jellyfin, observe)
}
