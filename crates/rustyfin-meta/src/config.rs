use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct MetaConfig {
    #[serde(default)]
    pub tmdb: TmdbConfig,
    #[serde(default)]
    pub tvdb: TvdbConfig,
    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct TmdbConfig {
    pub api_key: Option<String>,
    #[serde(default = "default_language")]
    #[allow(dead_code)]
    pub language: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TvdbConfig {
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub write_nfo: bool,
    #[serde(default = "default_true")]
    pub download_artwork: bool,
    #[serde(default)]
    pub overwrite_existing: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            write_nfo: true,
            download_artwork: true,
            overwrite_existing: false,
        }
    }
}

fn default_language() -> String {
    "en-US".to_string()
}

fn default_true() -> bool {
    true
}

impl MetaConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }
}
