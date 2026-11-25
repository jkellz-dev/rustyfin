//! The data layer: the only crate that talks to the database
//! (`CLAUDE.md`, "Layers").
//!
//! [`PgData`] implements the traits in `rustyfin_models::data` for
//! Postgres; database types and errors never leave this crate through
//! them. [`pool`] opens the connection pool and runs the migrations in
//! `migrations/`; [`repo`] has one module per table or concern (media
//! items, streams, user data, play history, sessions, libraries, people,
//! images, ...). Queries are plain SQL; the schema is documented in
//! `docs/database-schema.md`.
//!
//! Transitional: `pool` and `repo` are still public while callers move onto
//! the traits one area at a time; they become private once none is left.

mod pg;
pub mod pool;
pub mod repo;

pub use pg::{PgData, data_error};
pub use pool::DbPool;
