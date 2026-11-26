//! Who is asking: turning a request into a user and a session.
//!
//! Identity comes from a forward-auth proxy (Tailscale, Authelia, ...)
//! that Rustyfin trusts ([`forward_auth`], [`trusted_proxy`]), or, for apps
//! that sign in with a password, an LDAPS bind ([`ldap`]); Rustyfin stores
//! no passwords. [`middleware`] reads Jellyfin's `Authorization:
//! MediaBrowser ...` header (or `X-Emby-Token`, or `api_key`) into the
//! `AuthenticatedUser` extractor; [`session`] ties tokens to devices;
//! [`api_key`] covers server API keys.

pub mod api_key;
pub mod forward_auth;
pub mod ldap;
pub mod middleware;
pub mod session;
pub mod trusted_proxy;
