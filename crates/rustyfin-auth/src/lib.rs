//! Who is asking: turning a request into a user and a session.
//!
//! Identity comes only from a forward-auth proxy (Tailscale, Authelia, ...)
//! that Rustyfin trusts ([`forward_auth`], [`trusted_proxy`]); Rustyfin
//! checks no passwords and stores none. [`middleware`] reads Jellyfin's `Authorization:
//! MediaBrowser ...` header (or `X-Emby-Token`, or `api_key`) into the
//! `AuthenticatedUser` extractor; [`session`] ties tokens to devices;
//! [`api_key`] covers server API keys.

pub mod api_key;
pub mod forward_auth;
pub mod middleware;
pub mod session;
pub mod trusted_proxy;
