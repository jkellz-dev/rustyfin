use serde::Deserialize;

/// Rustyfin server configuration, loaded from TOML and/or environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub search: Option<SearchConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_server_name")]
    pub server_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_url")]
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MediaConfig {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SearchConfig {
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
}

fn default_server() -> ServerConfig {
    ServerConfig {
        port: default_port(),
        base_url: default_base_url(),
        server_name: default_server_name(),
    }
}

fn default_port() -> u16 {
    8096
}
fn default_base_url() -> String {
    "http://localhost:8096".to_string()
}
fn default_server_name() -> String {
    "Rustyfin".to_string()
}
fn default_database_url() -> String {
    "postgres://rustyfin:rustyfin@localhost/rustyfin".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
        }
    }
}

impl Config {
    /// Load config from TOML file and/or environment.
    /// Priority: env vars > TOML file > defaults.
    pub fn load() -> anyhow::Result<Self> {
        // Try loading TOML config file
        let config = if let Ok(path) = std::env::var("RUSTYFIN_CONFIG") {
            let contents = std::fs::read_to_string(&path)?;
            toml::from_str(&contents)?
        } else if std::path::Path::new("rustyfin.toml").exists() {
            let contents = std::fs::read_to_string("rustyfin.toml")?;
            toml::from_str(&contents)?
        } else {
            Config {
                server: default_server(),
                database: DatabaseConfig::default(),
                media: MediaConfig::default(),
                search: None,
            }
        };

        // Override with environment variables
        let config = Config {
            server: ServerConfig {
                port: std::env::var("RUSTYFIN_PORT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(config.server.port),
                base_url: std::env::var("RUSTYFIN_BASE_URL").unwrap_or(config.server.base_url),
                server_name: std::env::var("RUSTYFIN_SERVER_NAME")
                    .unwrap_or(config.server.server_name),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL").unwrap_or(config.database.url),
            },
            media: config.media,
            search: config.search,
        };

        Ok(config)
    }
}
