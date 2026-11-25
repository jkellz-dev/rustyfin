//! Shared models.
//!
//! [`data`] is the contract between services and the data layer: the traits
//! `rustyfin-data` implements, their models and their one error type.
//!
//! [`dto`] holds Jellyfin's request and response types (`BaseItemDto`,
//! `MediaSourceInfo`, `PlaybackInfoRequest`, session and user objects, ...)
//! serialized in Jellyfin's PascalCase, and [`enums`] the shared enumerations
//! (item kinds, image types, play methods). Both move to `rustyfin-jellyfin`
//! as the layered split goes on (`CLAUDE.md`, "Layers").

pub mod data;
pub mod dto;
pub mod enums;
