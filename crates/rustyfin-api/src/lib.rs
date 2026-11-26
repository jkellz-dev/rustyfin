//! Rustyfin's own API, under `/api/v1` (CLAUDE.md, "Layers"), apart from
//! the Jellyfin routes (owner, 2026-11-19: "I want any new api routes that
//! are rustyfin specific to be isolated from any of the old Jellyfin
//! routes"). It serialises and calls services; nothing here comes from
//! `rustyfin-jellyfin`.
//!
//! `/api` is free in Jellyfin: none of Jellyfin 10.10.7's 309 API paths
//! starts with it. Should a Jellyfin release take it, the prefix becomes
//! `/rfapi/v1` (owner, 2026-11-19).
//!
//! Paths are written in lower case and matched in any case ([`paths`]),
//! JSON and query keys are snake_case. Who is asking ([`caller`]) comes from
//! the trusted proxy in front; without one, from a session token. Today's
//! callers, the admin and linked Rustyfin servers, send the token the way
//! they do to the Jellyfin routes, so those forms are read here too, and ids
//! are written as 32 hex digits ([`ids`]), as before the API had its own
//! crate: what works today keeps working.
//!
//! [`router::build_router`] builds the routes; [`paths::owns`] tells the
//! server which requests are this API's.

pub mod caller;
pub mod error;
pub mod ids;
pub mod paths;
pub mod router;
pub mod routes;
pub mod state;
