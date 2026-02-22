use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct MetaConfig {
    #[serde(default)]
    pub tmdb: TmdbConfig,
    #[serde(default)]
    pub tvdb: TvdbConfig,
    #[serde(default)]
    pub musicbrainz: MusicBrainzConfig,
    #[serde(default)]
    pub fanart: FanartConfig,
    #[serde(default)]
    pub discogs: DiscogsConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub server: ServerConfig,
    /// Accepts the same `metadata.tmdb_api_key` field as the server config,
    /// so both binaries can share `RUSTYFIN__METADATA__TMDB_API_KEY`.
    #[serde(default)]
    pub metadata: SharedMetadataConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct SharedMetadataConfig {
    pub tmdb_api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TmdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_language")]
    #[allow(dead_code)]
    pub language: String,
    #[serde(default = "default_tmdb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
}

impl Default for TmdbConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            language: default_language(),
            rate_limit: default_tmdb_rate_limit(),
            rate_window_secs: default_rate_window(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TvdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_tvdb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
}

impl Default for TvdbConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            rate_limit: default_tvdb_rate_limit(),
            rate_window_secs: default_rate_window(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub write_nfo: bool,
    #[serde(default = "default_true")]
    pub download_artwork: bool,
    #[serde(default)]
    pub overwrite_existing: bool,
    #[serde(default = "default_people_cache_dir")]
    pub people_cache_dir: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            write_nfo: true,
            download_artwork: true,
            overwrite_existing: false,
            people_cache_dir: default_people_cache_dir(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MusicBrainzConfig {
    pub base_url: Option<String>,
    #[serde(default = "default_mb_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_mb_rate_window")]
    pub rate_window_secs: u64,
    #[serde(default = "default_mb_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "default_mb_cache_ttl")]
    pub cache_ttl_days: u32,
}

impl Default for MusicBrainzConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            rate_limit: default_mb_rate_limit(),
            rate_window_secs: default_mb_rate_window(),
            cache_dir: default_mb_cache_dir(),
            cache_ttl_days: default_mb_cache_ttl(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FanartConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_fanart_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_rate_window")]
    pub rate_window_secs: u64,
    #[serde(default = "default_max_backdrops")]
    pub max_backdrops: usize,
    #[serde(default = "default_max_per_type")]
    #[allow(dead_code)]
    pub max_per_type: usize,
}

impl Default for FanartConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            rate_limit: default_fanart_rate_limit(),
            rate_window_secs: default_rate_window(),
            max_backdrops: default_max_backdrops(),
            max_per_type: default_max_per_type(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DiscogsConfig {
    pub consumer_key: Option<String>,
    pub consumer_secret: Option<String>,
    #[serde(default = "default_discogs_rate_limit")]
    pub rate_limit: u32,
    #[serde(default = "default_discogs_rate_window")]
    pub rate_window_secs: u64,
    #[serde(default = "default_token_file")]
    pub token_file: String,
}

impl Default for DiscogsConfig {
    fn default() -> Self {
        Self {
            consumer_key: None,
            consumer_secret: None,
            rate_limit: default_discogs_rate_limit(),
            rate_window_secs: default_discogs_rate_window(),
            token_file: default_token_file(),
        }
    }
}

fn default_people_cache_dir() -> String {
    "~/.cache/rustyfin/people".to_string()
}

fn default_language() -> String {
    "en-US".to_string()
}

fn default_true() -> bool {
    true
}

fn default_tmdb_rate_limit() -> u32 {
    40
}

fn default_tvdb_rate_limit() -> u32 {
    20
}

fn default_mb_rate_limit() -> u32 {
    1
}

fn default_mb_rate_window() -> u64 {
    1
}

fn default_mb_cache_dir() -> String {
    "~/.cache/rustyfin/musicbrainz".to_string()
}

fn default_mb_cache_ttl() -> u32 {
    30
}

fn default_fanart_rate_limit() -> u32 {
    10
}

fn default_rate_window() -> u64 {
    10
}

fn default_max_backdrops() -> usize {
    5
}

fn default_max_per_type() -> usize {
    1
}

fn default_discogs_rate_limit() -> u32 {
    60
}

fn default_discogs_rate_window() -> u64 {
    60
}

fn default_token_file() -> String {
    "~/.config/rustyfin/discogs_tokens.json".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
}

impl MetaConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let builder = config::Config::builder()
            // Config file from CLI arg
            .add_source(
                config::File::new(
                    path.to_str().unwrap_or("rustyfin-meta.toml"),
                    config::FileFormat::Toml,
                )
                .required(false),
            )
            // Environment variables: RUSTYFIN__TMDB__API_KEY, RUSTYFIN__METADATA__TMDB_API_KEY, etc.
            .add_source(
                config::Environment::with_prefix("RUSTYFIN")
                    .separator("__")
                    .try_parsing(true),
            );

        let mut config: Self = builder.build()?.try_deserialize()?;

        // Allow the shared RUSTYFIN__METADATA__TMDB_API_KEY to fill tmdb.api_key
        if config.tmdb.api_key.is_none() {
            if let Some(key) = config.metadata.tmdb_api_key.take() {
                config.tmdb.api_key = Some(key);
            }
        }

        Ok(config)
    }
}
