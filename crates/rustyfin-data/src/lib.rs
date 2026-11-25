//! Postgres access with sqlx.
//!
//! [`pool`] opens the connection pool and runs the migrations in
//! `migrations/`; [`repo`] has one module per table or concern (media
//! items, streams, user data, play history, sessions, libraries, people,
//! images, ...). Queries are plain SQL; the schema is documented in
//! `docs/database-schema.md`.

pub mod pool;
pub mod repo;

pub use pool::DbPool;
