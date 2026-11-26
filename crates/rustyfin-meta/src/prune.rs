//! `rustyfin prune`: remove from the database the libraries `rustyfin.toml`
//! does not set (`[[library]]`), with everything in them (their titles,
//! everyone's watched history of them, who may see them). The files on disk
//! are not touched, and a linked server's libraries are the link's.
//!
//! It reads the server's own config file (its `[database]` and its
//! `[[library]]` entries) and `RUSTYFIN__*` environment variables, as the
//! server does. It lists what would go; `--yes` removes it.

use std::path::Path;

use rustyfin_data::DatabaseConfig;
use rustyfin_services::libraries::{ConfiguredLibrary, Libraries};
use serde::Deserialize;

/// What prune reads from the server's config file.
#[derive(Debug, Default, Deserialize)]
pub struct ServerFile {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default, rename = "library")]
    pub libraries: Vec<ConfiguredLibrary>,
}

impl ServerFile {
    /// The server's config file, then its `RUSTYFIN__*` variables.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::from(path).format(config::FileFormat::Toml))
            .add_source(
                config::Environment::with_prefix("RUSTYFIN")
                    .prefix_separator("__")
                    .separator("__"),
            )
            .build()
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
        settings
            .try_deserialize()
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
    }
}

/// List (or with `apply`, remove) the libraries the server's config does not set.
pub async fn run(server_config: &Path, apply: bool) -> anyhow::Result<()> {
    let file = ServerFile::load(server_config)?;
    let pool = rustyfin_data::pool::create_pool(&file.database.url())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "cannot reach the database at {}:{}/{}: {e}",
                file.database.host,
                file.database.port,
                file.database.name
            )
        })?;
    let data = rustyfin_data::PgData::from_pool(&pool);
    let stray = Libraries::over(&data)
        .prune(&file.libraries, apply)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if stray.is_empty() {
        tracing::info!("Every library is set in {}", server_config.display());
        return Ok(());
    }
    for lib in &stray {
        let folders = lib.paths.join(", ");
        if apply {
            tracing::info!("Removed library {} ({folders})", lib.name);
        } else {
            tracing::info!("Not in the config: library {} ({folders})", lib.name);
        }
    }
    if !apply {
        tracing::info!(
            "{} libraries would be removed, with their titles and everyone's watched \
             history of them; run again with --yes to remove them",
            stray.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_file_gives_the_database_and_libraries() {
        let dir = std::env::temp_dir().join(format!("rustyfin-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rustyfin.toml");
        std::fs::write(
            &path,
            r#"
[server]
port = 8096

[database]
host = "postgres"
name = "media"

[[library]]
name = "Movies"
type = "movies"
paths = ["/data/media/movies", "/mnt/media/movies"]

[[link]]
name = "Friend"
"#,
        )
        .unwrap();
        let file = ServerFile::load(&path).unwrap();
        assert_eq!(file.database.host, "postgres");
        assert_eq!(file.database.name, "media");
        assert_eq!(file.database.port, 5432);
        assert_eq!(
            file.libraries,
            [ConfiguredLibrary {
                name: "Movies".into(),
                kind: "movies".into(),
                paths: vec!["/data/media/movies".into(), "/mnt/media/movies".into()],
            }]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_named() {
        let err = ServerFile::load(Path::new("/nowhere/rustyfin.toml"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("/nowhere/rustyfin.toml"), "{err}");
    }
}
