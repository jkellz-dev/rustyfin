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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// The device name Rustyfin's link client signs in with.
pub const LINK_DEVICE: &str = "Rustyfin link";

/// How long whether a token is a linking server's is remembered.
const REMEMBER: Duration = Duration::from_secs(60);

/// A pool like `pool` whose connections leave out what links hold.
pub fn local_only_pool(pool: &PgPool) -> PgPool {
    let options = (*pool.connect_options())
        .clone()
        .options([("rustyfin.local_only", "on")]);
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(rustyfin_db::pool::ACQUIRE_TIMEOUT)
        .connect_lazy_with(options)
}

/// What [`dispatch`] needs: the full pool to look tokens up, and the routes
/// for linking servers.
#[derive(Clone)]
pub struct LinkScope {
    pub pool: PgPool,
    pub local_routes: Router,
    seen: Arc<Mutex<HashMap<String, (bool, Instant)>>>,
}

impl LinkScope {
    pub fn new(pool: PgPool, local_routes: Router) -> Self {
        Self {
            pool,
            local_routes,
            seen: Arc::default(),
        }
    }

    /// Whether `token` is a linking server's (remembered a minute).
    async fn is_linking(&self, token: &str) -> bool {
        let now = Instant::now();
        if let Some((linking, at)) = self.seen.lock().unwrap().get(token)
            && now.duration_since(*at) < REMEMBER
        {
            return *linking;
        }
        let linking = match rustyfin_auth::session::resolve_token(&self.pool, token).await {
            Ok(r) => r.user.may_link || r.session.device_name == LINK_DEVICE,
            // Unknown or failing: the routes answer as they would.
            Err(_) => return false,
        };
        let mut seen = self.seen.lock().unwrap();
        if seen.len() > 10_000 {
            seen.retain(|_, (_, at)| now.duration_since(*at) < REMEMBER);
        }
        seen.insert(token.to_string(), (linking, now));
        linking
    }
}

/// Middleware before routing: a linking server's request goes to the
/// local-only routes.
pub async fn dispatch(State(scope): State<LinkScope>, req: Request, next: Next) -> Response {
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
