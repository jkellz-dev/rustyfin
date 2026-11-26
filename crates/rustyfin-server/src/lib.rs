//! The server: what the `rustyfin-server` binary puts together, shared with
//! the tests so they run the same composition.
//!
//! [`app`] mounts the API routers (Rustyfin's own, `rustyfin-api`, and
//! Jellyfin's, `rustyfin-jellyfin`); around every route the server applies
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

/// The API: Rustyfin's own routes (`rustyfin-api`) for the paths that are
/// theirs, the Jellyfin routes for the rest; each route observed
/// ([`observe`]).
pub fn app<D: Data>(
    jellyfin: rustyfin_jellyfin::router::AppState<D>,
    api: rustyfin_api::state::ApiState<D>,
) -> Router {
    let api = rustyfin_api::router::build_router(api, observe);
    rustyfin_jellyfin::router::build_router(jellyfin, observe)
        .layer(axum::middleware::from_fn_with_state(api, to_own_api))
}

/// Middleware in front of the Jellyfin routes: a request for Rustyfin's own
/// API goes there instead.
async fn to_own_api(
    axum::extract::State(api): axum::extract::State<Router>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tower::ServiceExt;
    if rustyfin_api::paths::owns(req.uri().path()) {
        return match api.oneshot(req).await {
            Ok(response) => response,
            Err(never) => match never {},
        };
    }
    next.run(req).await
}
