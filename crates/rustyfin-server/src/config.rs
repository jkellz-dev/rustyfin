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
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// Linked servers (`[[link]]`, roadmap 16b; docs/linked-servers.md).
    #[serde(default, rename = "link")]
    pub links: Vec<LinkConfig>,
}

/// One `[[link]]`: a friend's server whose shared libraries merge into this
/// one's.
#[derive(Clone, Deserialize)]
pub struct LinkConfig {
    pub name: String,
    pub kind: rustyfin_core::link::LinkKind,
    pub url: String,
    /// The link user (Jellyfin). A Rustyfin needs none: its forward-auth
    /// proxy says who this server is.
    #[serde(default)]
    pub user: String,
    /// The link user's password, or `password_file` naming a file that holds
    /// it (a mounted secret). Jellyfin only.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub password_file: Option<String>,
    #[serde(default)]
    pub libraries: Vec<rustyfin_core::link::LibraryMap>,
}

impl std::fmt::Debug for LinkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkConfig")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("url", &self.url)
            .field("user", &self.user)
            .field("password_file", &self.password_file)
            .field("libraries", &self.libraries)
            .finish_non_exhaustive()
    }
}

impl LinkConfig {
    /// The settings the link runs on, its password read.
    pub fn settings(&self) -> anyhow::Result<rustyfin_core::link::LinkSettings> {
        let password = match (&self.password, &self.password_file) {
            (Some(p), None) => p.clone(),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("link {}: cannot read {file}: {e}", self.name))?
                .trim_end_matches(['\r', '\n'])
                .to_string(),
            (None, None) if self.kind == rustyfin_core::link::LinkKind::Rustyfin => String::new(),
            _ => anyhow::bail!("link {}: set one of password and password_file", self.name),
        };
        if self.kind == rustyfin_core::link::LinkKind::Jellyfin && self.user.trim().is_empty() {
            anyhow::bail!(
                "link {}: name the user the friend made for the link",
                self.name
            );
        }
        if self.libraries.is_empty() {
            anyhow::bail!("link {}: name at least one library to merge", self.name);
        }
        Ok(rustyfin_core::link::LinkSettings {
            name: self.name.clone(),
            kind: self.kind,
            url: self.url.clone(),
            user: self.user.clone(),
            password,
            libraries: self.libraries.clone(),
        })
    }
}

/// Prometheus metrics, on a listener of their own (not the public port).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetricsConfig {
    /// Address for `GET /metrics`, e.g. `0.0.0.0:9090`. Unset: no metrics.
    #[serde(default)]
    pub listen: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_server_name")]
    pub server_name: String,
    #[serde(default)]
    pub user_image_url_template: Option<String>,
    #[serde(default)]
    pub image_cache_dir: Option<String>,
    /// Origins of web clients served from elsewhere that may call the API.
    /// Empty (the default): same-origin only.
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
}
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

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    pub meilisearch_url: String,
    pub meilisearch_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetadataConfig {
    pub tmdb_api_key: Option<String>,
    pub people_cache_dir: Option<String>,
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
    /// A Tailscale app capability whose grant names the user when a request
    /// carries no user header (a tagged node, such as a linked server).
    #[serde(default)]
    pub app_capability: Option<String>,
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
    #[serde(default = "default_app_capabilities")]
    pub app_capabilities: String,
}

fn default_app_capabilities() -> String {
    "Tailscale-App-Capabilities".to_string()
}

impl Default for ForwardAuthHeadersConfig {
    fn default() -> Self {
        Self {
            user: default_remote_user(),
            name: default_remote_name(),
            email: default_remote_email(),
            groups: default_remote_groups(),
            app_capabilities: default_app_capabilities(),
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
    #[serde(default)]
    pub activity_log_prune: ActivityLogPruneConfig,
    #[serde(default)]
    pub session_cleanup: SessionCleanupConfig,
    #[serde(default)]
    pub transcode_temp_cleanup: TranscodeTempCleanupConfig,
    #[serde(default)]
    pub db_optimize: DbOptimizeConfig,
    #[serde(default)]
    pub image_cache_prune: ImageCachePruneConfig,
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
        schedules.insert(
            "weekly".to_string(),
            ScheduleEntry {
                cron: "0 0 4 * * 7 *".to_string(),
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
            activity_log_prune: ActivityLogPruneConfig::default(),
            session_cleanup: SessionCleanupConfig::default(),
            transcode_temp_cleanup: TranscodeTempCleanupConfig::default(),
            db_optimize: DbOptimizeConfig::default(),
            image_cache_prune: ImageCachePruneConfig::default(),
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

#[derive(Debug, Clone, Deserialize)]
pub struct ActivityLogPruneConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for ActivityLogPruneConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
            retention_days: default_retention_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionCleanupConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
    #[serde(default = "default_inactive_days")]
    pub inactive_days: u32,
}

impl Default for SessionCleanupConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
            inactive_days: default_inactive_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscodeTempCleanupConfig {
    #[serde(default = "default_maintenance_daily")]
    pub schedule: String,
}

impl Default for TranscodeTempCleanupConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_daily(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbOptimizeConfig {
    #[serde(default = "default_maintenance_weekly")]
    pub schedule: String,
}

impl Default for DbOptimizeConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_weekly(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageCachePruneConfig {
    #[serde(default = "default_maintenance_weekly")]
    pub schedule: String,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: u32,
}

impl Default for ImageCachePruneConfig {
    fn default() -> Self {
        Self {
            schedule: default_maintenance_weekly(),
            max_age_days: default_max_age_days(),
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
fn default_maintenance_daily() -> String {
    "daily".to_string()
}
fn default_maintenance_weekly() -> String {
    "weekly".to_string()
}
fn default_retention_days() -> u32 {
    90
}
fn default_inactive_days() -> u32 {
    30
}
fn default_max_age_days() -> u32 {
    30
}

// ── Defaults ─────────────────────────────────────────────────────────

fn default_server() -> ServerConfig {
    ServerConfig {
        port: default_port(),
        base_url: default_base_url(),
        server_name: default_server_name(),
        user_image_url_template: None,
        cors_allowed_origins: Vec::new(),
        image_cache_dir: None,
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

impl Config {
    /// Load config from TOML file and/or environment.
    /// Priority: env vars > TOML file > defaults.
    pub fn load() -> anyhow::Result<Self> {
        let mut builder = config::Config::builder()
            // Default config file (rustyfin.toml in cwd)
            .add_source(config::File::with_name("rustyfin").required(false));

        // Explicit config file path from env
        if let Ok(path) = std::env::var("RUSTYFIN_CONFIG") {
            builder = builder
                .add_source(config::File::new(&path, config::FileFormat::Toml).required(true));
        }

        // Environment variables: RUSTYFIN__SERVER__PORT, RUSTYFIN__DATABASE__HOST, etc.
        // Note: Kubernetes injects service-discovery env vars like RUSTYFIN_PORT,
        // RUSTYFIN_SERVICE_HOST, etc. when a Service named "rustyfin" exists.
        // Using "__" separator ensures only double-underscore vars are parsed,
        // but we also set prefix_separator to "__" to avoid single-underscore
        // k8s vars from being picked up.
        builder = builder.add_source(
            config::Environment::with_prefix("RUSTYFIN")
                .prefix_separator("__")
                .separator("__")
                .try_parsing(true),
        );

        let config: Config = builder.build()?.try_deserialize()?;
        Self::validate_auth(&config.auth)?;
        Ok(config)
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
