//! Shared models.
//!
//! [`data`] is the contract between services and the data layer: the traits
//! `rustyfin-data` implements, their models and their one error type.
//!
//! Jellyfin's request and response types are `rustyfin-jellyfin`'s own.

pub mod data;
