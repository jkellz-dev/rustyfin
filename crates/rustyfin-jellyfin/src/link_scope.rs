//! A server linking this one sees only this server's own media
//! (`rustyfin_services::link::scope`). Such a request is served by a second
//! copy of the routes whose database connections set `rustyfin.local_only`,
//! which `visible_media_items` reads: every row held from a link is left out
//! there, in every view at once (lists, search, Latest, a title's versions,
//! an item by id). Libraries made for a friend's library are left out too,
//! and the relay to friends answers 404.
use rustyfin_models::data::Data;

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;

/// What [`dispatch`] needs: the full data layer to look tokens up, and the routes
/// for linking servers.
#[derive(Clone)]
pub struct LinkScope<D> {
    pub data: D,
    pub local_routes: Router,
}

impl<D: Data> LinkScope<D> {
    pub fn new(data: D, local_routes: Router) -> Self {
        Self { data, local_routes }
    }
}

/// Middleware before routing: a linking server's request goes to the
/// local-only routes.
pub async fn dispatch<D: Data>(
    State(scope): State<LinkScope<D>>,
    req: Request,
    next: Next,
) -> Response {
    let token = crate::auth::middleware::request_token(req.headers(), req.uri().query());
    if let Some(token) = token
        && rustyfin_services::link::scope::is_linking(&scope.data, &token).await
    {
        return match scope.local_routes.clone().oneshot(req).await {
            Ok(response) => response,
            Err(never) => match never {},
        }
        .into_response();
    }
    next.run(req).await
}
