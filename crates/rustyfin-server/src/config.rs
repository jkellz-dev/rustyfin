use std::collections::HashMap;

use serde::Deserialize;

/// Rustyfin server configuration, loaded from TOML and/or environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub search: Option<SearchConfig>,
    #[serde(default)]
    pub metadata: MetadataConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub transcode: Option<TranscodeConfig>,
    #[serde(default)]
    pub tasks: TasksConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetadataConfig {
    pub tmdb_api_key: Option<String>,
}

// ── Transcode configuration ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TranscodeConfig {
    /// Path to ffmpeg binary (default: "ffmpeg" from PATH)
    #[serde(default = "default_ffmpeg_path")]
    pub ffmpeg_path: String,
    /// Directory for HLS segment output (default: system temp dir)
    pub temp_dir: Option<String>,
    /// Hardware acceleration: "none", "qsv", "vaapi", "nvenc" (default: "none")
    #[serde(default = "default_hw_accel")]
    pub hardware_acceleration: String,
    /// Max concurrent transcode jobs (default: 4, 0 = unlimited)
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Default segment duration in seconds (default: 6)
    #[serde(default = "default_segment_duration")]
    pub segment_duration: u32,
    /// Threads per ffmpeg process (default: 0 = auto)
    #[serde(default)]
    pub threads_per_job: u32,
}

impl Default for TranscodeConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: default_ffmpeg_path(),
            temp_dir: None,
            hardware_acceleration: default_hw_accel(),
            max_concurrent: default_max_concurrent(),
            segment_duration: default_segment_duration(),
            threads_per_job: 0,
        }
    }
}

fn default_ffmpeg_path() -> String {
    "ffmpeg".to_string()
}
fn default_hw_accel() -> String {
    "none".to_string()
}
fn default_max_concurrent() -> usize {
    4
}
fn default_segment_duration() -> u32 {
    6
}

// ── Auth configuration ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub forward_auth: Option<ForwardAuthConfig>,
    #[serde(default)]
    pub ldap: Option<LdapAuthConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForwardAuthConfig {
    /// CIDR ranges of trusted proxies (e.g. `["10.0.0.0/8", "127.0.0.1/32"]`).
    pub trusted_proxies: Vec<String>,
    /// Auto-admin users whose groups header contains this value.
    pub admin_group: Option<String>,
    /// Configurable header names.
    #[serde(default)]
    pub headers: ForwardAuthHeadersConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForwardAuthHeadersConfig {
    #[serde(default = "default_remote_user")]
    pub user: String,
    #[serde(default = "default_remote_name")]
    pub name: String,
    #[serde(default = "default_remote_email")]
    pub email: String,
    #[serde(default = "default_remote_groups")]
    pub groups: String,
}

impl Default for ForwardAuthHeadersConfig {
    fn default() -> Self {
        Self {
            user: default_remote_user(),
            name: default_remote_name(),
            email: default_remote_email(),
            groups: default_remote_groups(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LdapAuthConfig {
    /// LDAPS URL (must start with `ldaps://`).
    pub url: String,
    /// Service account DN for searching users.
    pub bind_dn: String,
    /// Service account password.
    pub bind_password: String,
    /// Base DN for user searches.
    pub search_base: String,
    /// LDAP filter with `{}` placeholder for username.
    #[serde(default = "default_user_filter")]
    pub user_filter: String,
    /// Attribute for email.
    #[serde(default = "default_mail_attr")]
    pub mail_attr: String,
    /// Attribute for display name.
    #[serde(default = "default_display_name_attr")]
    pub display_name_attr: String,
    /// Group DN whose members are auto-promoted to admin.
    pub admin_group_dn: Option<String>,
}

fn default_remote_user() -> String {
    "Remote-User".to_string()
}
fn default_remote_name() -> String {
    "Remote-Name".to_string()
}
fn default_remote_email() -> String {
    "Remote-Email".to_string()
}
fn default_remote_groups() -> String {
    "Remote-Groups".to_string()
}
fn default_user_filter() -> String {
    "(uid={})".to_string()
}
fn default_mail_attr() -> String {
    "mail".to_string()
}
fn default_display_name_attr() -> String {
    "cn".to_string()
}

// ── Task scheduler configuration ─────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TasksConfig {
    #[serde(default = "default_tasks_enabled")]
    pub enabled: bool,
    #[serde(default = "default_run_on_startup")]
    pub run_on_startup: bool,
    #[serde(default = "default_jitter_seconds")]
    pub jitter_seconds: u64,
    #[serde(default)]
    pub schedules: HashMap<String, ScheduleEntry>,
    #[serde(default)]
    pub library_scan: LibraryScanConfig,
    #[serde(default)]
    pub search_reindex: SearchReindexConfig,
}

impl Default for TasksConfig {
    fn default() -> Self {
        let mut schedules = HashMap::new();
        schedules.insert(
            "daily".to_string(),
            ScheduleEntry {
                cron: "0 0 3 * * * *".to_string(),
                jitter_seconds: None,
            },
        );
        Self {
            enabled: true,
            run_on_startup: true,
            jitter_seconds: 300,
            schedules,
            library_scan: LibraryScanConfig::default(),
            search_reindex: SearchReindexConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleEntry {
    pub cron: String,
    pub jitter_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryScanConfig {
    #[serde(default = "default_library_scan_schedule")]
    pub default_schedule: String,
    #[serde(default)]
    pub library_schedules: HashMap<String, String>,
}

impl Default for LibraryScanConfig {
    fn default() -> Self {
        Self {
            default_schedule: default_library_scan_schedule(),
            library_schedules: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchReindexConfig {
    #[serde(default = "default_search_reindex_schedule")]
    pub schedule: String,
    #[serde(default)]
    pub jitter_seconds: Option<u64>,
}

impl Default for SearchReindexConfig {
    fn default() -> Self {
        Self {
            schedule: default_search_reindex_schedule(),
            jitter_seconds: Some(0),
        }
    }
}

fn default_tasks_enabled() -> bool {
    true
}
fn default_run_on_startup() -> bool {
    true
}
fn default_jitter_seconds() -> u64 {
    300
}
fn default_library_scan_schedule() -> String {
    "daily".to_string()
}
fn default_search_reindex_schedule() -> String {
    "daily".to_string()
}

// ── Defaults ─────────────────────────────────────────────────────────

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
                search: None,
                metadata: MetadataConfig::default(),
                auth: AuthConfig::default(),
                transcode: None,
                tasks: TasksConfig::default(),
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
            search: config.search,
            metadata: MetadataConfig {
                tmdb_api_key: std::env::var("RUSTYFIN_TMDB_API_KEY")
                    .ok()
                    .or(config.metadata.tmdb_api_key),
            },
            auth: Self::apply_auth_env_overrides(config.auth),
            transcode: config.transcode,
            tasks: config.tasks,
        };

        // Validate auth configuration
        Self::validate_auth(&config.auth)?;

        Ok(config)
    }

    /// Apply environment variable overrides to auth config.
    fn apply_auth_env_overrides(mut auth: AuthConfig) -> AuthConfig {
        // LDAP env overrides
        if let Ok(url) = std::env::var("RUSTYFIN_LDAP_URL") {
            let ldap = auth.ldap.get_or_insert_with(|| LdapAuthConfig {
                url: String::new(),
                bind_dn: String::new(),
                bind_password: String::new(),
                search_base: String::new(),
                user_filter: default_user_filter(),
                mail_attr: default_mail_attr(),
                display_name_attr: default_display_name_attr(),
                admin_group_dn: None,
            });
            ldap.url = url;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_LDAP_BIND_DN")
            && let Some(ref mut ldap) = auth.ldap
        {
            ldap.bind_dn = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_LDAP_BIND_PASSWORD")
            && let Some(ref mut ldap) = auth.ldap
        {
            ldap.bind_password = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_LDAP_SEARCH_BASE")
            && let Some(ref mut ldap) = auth.ldap
        {
            ldap.search_base = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_LDAP_USER_FILTER")
            && let Some(ref mut ldap) = auth.ldap
        {
            ldap.user_filter = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_LDAP_ADMIN_GROUP_DN")
            && let Some(ref mut ldap) = auth.ldap
        {
            ldap.admin_group_dn = Some(v);
        }

        // Trusted proxies env override
        if let Ok(v) = std::env::var("RUSTYFIN_TRUSTED_PROXIES") {
            let proxies: Vec<String> = v.split(',').map(|s| s.trim().to_string()).collect();
            let fa = auth.forward_auth.get_or_insert_with(|| ForwardAuthConfig {
                trusted_proxies: Vec::new(),
                admin_group: None,
                headers: ForwardAuthHeadersConfig::default(),
            });
            fa.trusted_proxies = proxies;
        }

        // Forward auth header env overrides
        if let Ok(v) = std::env::var("RUSTYFIN_FORWARD_AUTH_HEADER_USER")
            && let Some(ref mut fa) = auth.forward_auth
        {
            fa.headers.user = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_FORWARD_AUTH_HEADER_NAME")
            && let Some(ref mut fa) = auth.forward_auth
        {
            fa.headers.name = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_FORWARD_AUTH_HEADER_EMAIL")
            && let Some(ref mut fa) = auth.forward_auth
        {
            fa.headers.email = v;
        }
        if let Ok(v) = std::env::var("RUSTYFIN_FORWARD_AUTH_HEADER_GROUPS")
            && let Some(ref mut fa) = auth.forward_auth
        {
            fa.headers.groups = v;
        }

        auth
    }

    /// Validate auth configuration at startup.
    fn validate_auth(auth: &AuthConfig) -> anyhow::Result<()> {
        // At least one auth method must be configured
        if auth.forward_auth.is_none() && auth.ldap.is_none() {
            anyhow::bail!(
                "At least one auth method must be configured ([auth.forward_auth] or [auth.ldap])"
            );
        }

        // LDAP URL must be ldaps://
        if let Some(ref ldap) = auth.ldap
            && !ldap.url.starts_with("ldaps://")
        {
            anyhow::bail!("LDAP URL must use ldaps:// (STARTTLS is not supported)");
        }

        // Trusted proxies must parse as valid CIDRs
        if let Some(ref fa) = auth.forward_auth {
            for cidr in &fa.trusted_proxies {
                cidr.parse::<ipnet::IpNet>()
                    .map_err(|e| anyhow::anyhow!("Invalid trusted proxy CIDR '{cidr}': {e}"))?;
            }
        }

        Ok(())
    }
}
