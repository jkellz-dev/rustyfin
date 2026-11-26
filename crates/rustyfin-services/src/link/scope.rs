//! A server linking this one sees only this server's own media (owner,
//! 2026-11-19: "Only users who are directly using my instance of rustyfin
//! should see linked libraries"). Without it, two friends linking each other
//! would hand each other's titles back and forth, and a friend of a friend
//! would see media they were never given.
//!
//! A request is a linking server's when its session belongs to a user the
//! admin let link ("Allow linking", `users.may_link`) or was opened by
//! Rustyfin's own link client (device [`LINK_DEVICE`]). The APIs serve such
//! a request from `Data::local_only()`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use rustyfin_models::data::Data;

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

/// Whether `token` is a linking server's (remembered, see [`SEEN`]). An
/// unknown or failing token is not: the routes answer as they would.
pub async fn is_linking<D: Data>(data: &D, token: &str) -> bool {
    if let Some(linking) = SEEN.lock().unwrap().get(token) {
        return *linking;
    }
    let linking = match crate::tokens::resolve_token(data, token).await {
        Ok(r) => r.user.may_link || r.session.device_name == LINK_DEVICE,
        Err(_) => return false,
    };
    let mut seen = SEEN.lock().unwrap();
    if seen.len() >= MAX_SEEN {
        seen.clear();
    }
    seen.insert(token.to_string(), linking);
    linking
}
