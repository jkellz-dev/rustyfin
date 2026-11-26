//! A server linking this one sees only this server's own media (owner,
//! 2026-11-19: "Only users who are directly using my instance of rustyfin
//! should see linked libraries"). Without it, two friends linking each other
//! would hand each other's titles back and forth, and a friend of a friend
//! would see media they were never given.
//!
//! A request is a linking server's when its session belongs to a user the
//! admin let link ("Allow linking", `users.may_link`) or was opened by
//! Rustyfin's own link client (device "Rustyfin link"). Such a request is
//! served by a second copy of the routes whose database connections set
//! `rustyfin.local_only`, which `visible_media_items` reads: every row held
//! from a link is left out there, in every view at once (lists, search,
//! Latest, a title's versions, an item by id). Libraries made for a friend's
//! library are left out too, and the relay to friends answers 404.
use rustyfin_models::data::Data;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;

/// The device name Rustyfin's link client signs in with.
pub const LINK_DEVICE: &str = "Rustyfin link";

/// Whether a token is a linking server's, once looked up. A session's user
/// and device never change; a user's `may_link` does, and [`forget`] is
/// called when it is set. No expiry, so the answer for a request never
/// depends on timing.
static SEEN: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(Mutex::default);

/// Tokens remembered at most; past it the memory starts again.
const MAX_SEEN: usize = 10_000;

/// Forget what is remembered (a user's `may_link` changed).
pub fn forget() {
    SEEN.lock().unwrap().clear();
}

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

    /// Whether `token` is a linking server's (remembered, see [`SEEN`]).
    async fn is_linking(&self, token: &str) -> bool {
        if let Some(linking) = SEEN.lock().unwrap().get(token) {
            return *linking;
        }
        let linking = match rustyfin_auth::session::resolve_token(&self.data, token).await {
            Ok(r) => r.user.may_link || r.session.device_name == LINK_DEVICE,
            // Unknown or failing: the routes answer as they would.
            Err(_) => return false,
        };
        let mut seen = SEEN.lock().unwrap();
        if seen.len() >= MAX_SEEN {
            seen.clear();
        }
        seen.insert(token.to_string(), linking);
        linking
    }
}

/// Middleware before routing: a linking server's request goes to the
/// local-only routes.
pub async fn dispatch<D: Data>(
    State(scope): State<LinkScope<D>>,
    req: Request,
    next: Next,
) -> Response {
    let token = rustyfin_auth::middleware::request_token(req.headers(), req.uri().query());
    if let Some(token) = token
        && scope.is_linking(&token).await
    {
        return match scope.local_routes.clone().oneshot(req).await {
            Ok(response) => response,
            Err(never) => match never {},
        }
        .into_response();
    }
    next.run(req).await
}
