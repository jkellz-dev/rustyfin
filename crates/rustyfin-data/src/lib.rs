//! The data layer: the only crate that talks to the database
//! (`CLAUDE.md`, "Layers").
//!
//! [`PgData`] implements the traits in `rustyfin_models::data` for
//! Postgres; database types and errors never leave this crate through
//! them. [`pool`] opens the connection pool and runs the migrations in
//! `migrations/`; [`repo`] has one module per table or concern (media
//! items, streams, user data, play history, sessions, libraries, people,
//! images, ...). Queries are plain SQL; the schema is documented in
//! `docs/database-schema.md`. The `search` module is the Meilisearch index
//! that ranks searches, with Postgres as its fallback ([`PgSearch`]).
//!
//! Transitional: `pool` and `repo` are still public while callers move onto
//! the traits one area at a time; they become private once none is left.

mod config;
mod pg;
pub mod pool;
pub mod repo;
mod search;

pub use config::DatabaseConfig;
pub use pg::{PgData, data_error};
pub use pool::DbPool;
pub use search::PgSearch;
