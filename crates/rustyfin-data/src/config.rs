//! `[database]` in `rustyfin.toml`: where Postgres is (the server and
//! `rustyfin prune` both connect with it).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_host")]
    pub host: String,
    #[serde(default = "default_db_port")]
    pub port: u16,
    #[serde(default = "default_db_user")]
    pub user: String,
    #[serde(default = "default_db_password")]
    pub password: String,
    #[serde(default = "default_db_name")]
    pub name: String,
}

impl DatabaseConfig {
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }
}

fn default_db_host() -> String {
    "localhost".to_string()
}
fn default_db_port() -> u16 {
    5432
}
fn default_db_user() -> String {
    "rustyfin".to_string()
}
fn default_db_password() -> String {
    "rustyfin".to_string()
}
fn default_db_name() -> String {
    "rustyfin".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: default_db_host(),
            port: default_db_port(),
            user: default_db_user(),
            password: default_db_password(),
            name: default_db_name(),
        }
    }
}
