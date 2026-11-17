//! The shapes on the wire: Jellyfin's JSON objects as Rust types.
//!
//! [`dto`] holds the request and response types (`BaseItemDto`,
//! `MediaSourceInfo`, `PlaybackInfoRequest`, session and user objects, ...)
//! serialized in Jellyfin's PascalCase; [`enums`] the shared enumerations
//! (item kinds, image types, play methods).

pub mod dto;
pub mod enums;
