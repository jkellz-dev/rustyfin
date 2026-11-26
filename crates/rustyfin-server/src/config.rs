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
    #[serde(default)]
    pub tracing: TracingConfig,
    /// The libraries (`[[library]]`; docs/configuration.md): made or
    /// updated at start, and changed only here once any is set.
    #[serde(default, rename = "library")]
    pub libraries: Vec<rustyfin_services::libraries::ConfiguredLibrary>,
    /// Linked servers (`[[link]]`, roadmap 16b; docs/linked-servers.md).
    #[serde(default, rename = "link")]
    pub links: Vec<LinkConfig>,
    #[serde(default)]
    pub playback: PlaybackConfig,
}

/// One `[[link]]`: a friend's server whose shared libraries merge into this
/// one's.
#[derive(Clone, Deserialize)]
pub struct LinkConfig {
    pub name: String,
    pub kind: rustyfin_services::link::LinkKind,
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
    pub libraries: Vec<rustyfin_services::link::LibraryMap>,
    /// A client-credentials token for the friend's proxy (`[link.oidc]`).
    #[serde(default)]
    pub oidc: Option<LinkOidcConfig>,
}

/// `[link.oidc]`: the friend's proxy takes OIDC tokens; this server fetches
/// one from the friend's identity provider (OAuth 2.0 client credentials).
#[derive(Clone, Deserialize)]
pub struct LinkOidcConfig {
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub client_secret_file: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
}

impl LinkOidcConfig {
    fn settings(&self, link: &str) -> anyhow::Result<rustyfin_services::link::OidcSettings> {
        if self.issuer.is_none() && self.token_url.is_none() {
            anyhow::bail!("link {link}: [link.oidc] needs issuer or token_url");
        }
        let client_secret = match (&self.client_secret, &self.client_secret_file) {
            (Some(s), None) => s.clone(),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("link {link}: cannot read {file}: {e}"))?
                .trim_end_matches(['\r', '\n'])
                .to_string(),
            _ => anyhow::bail!("link {link}: set one of client_secret and client_secret_file"),
        };
        Ok(rustyfin_services::link::OidcSettings {
            issuer: self.issuer.clone(),
            token_url: self.token_url.clone(),
            client_id: self.client_id.clone(),
            client_secret,
            scope: self.scope.clone(),
            audience: self.audience.clone(),
        })
    }
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
    pub fn settings(&self) -> anyhow::Result<rustyfin_services::link::LinkSettings> {
        let password = match (&self.password, &self.password_file) {
            (Some(p), None) => p.clone(),
            (None, Some(file)) => std::fs::read_to_string(file)
                .map_err(|e| anyhow::anyhow!("link {}: cannot read {file}: {e}", self.name))?
                .trim_end_matches(['\r', '\n'])
                .to_string(),
            (None, None) if self.kind == rustyfin_services::link::LinkKind::Rustyfin => {
                String::new()
            }
            _ => anyhow::bail!("link {}: set one of password and password_file", self.name),
        };
        if self.kind == rustyfin_services::link::LinkKind::Jellyfin && self.user.trim().is_empty() {
            anyhow::bail!(
                "link {}: name the user the friend made for the link",
                self.name
            );
        }
        if self.libraries.is_empty() {
            anyhow::bail!("link {}: name at least one library to merge", self.name);
        }
        Ok(rustyfin_services::link::LinkSettings {
            name: self.name.clone(),
            kind: self.kind,
            url: self.url.clone(),
            user: self.user.clone(),
            password,
            libraries: self.libraries.clone(),
            oidc: self
                .oidc
                .as_ref()
                .map(|o| o.settings(&self.name))
                .transpose()?,
        })
    }
}

/// Logs and traces (`[tracing]`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TracingConfig {
    /// An OpenTelemetry collector's OTLP/HTTP address, e.g.
    /// `http://collector:4318`. Unset: no traces.
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    /// Log lines as JSON objects, each with its trace id when traced.
    #[serde(default)]
    pub json_logs: bool,
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
/// `[database]`: kept with the data layer, so every binary that connects
/// reads it the same way.
pub use rustyfin_data::DatabaseConfig;

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

// ── Playback ─────────────────────────────────────────────────────────

/// `[playback]`: how a title's copies are offered.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlaybackConfig {
    /// Of a title's copies, offer first those the app can play as is (no
    /// conversion) over better ones that would be converted. Off: the best
    /// copy comes first.
    #[serde(default)]
    pub prefer_direct_play: bool,
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
    /// Password sign-in against LDAPS was removed: only read to refuse a
    /// config that still asks for it.
    #[serde(default)]
    pub ldap: Option<serde::de::IgnoredAny>,
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

// ── Task scheduler configuration ─────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TasksConfig {
    #[serde(default = "default_tasks_enabled")]
    pub enabled: bool,
    #[serde(default = "default_run_on_startup")]
    pub run_on_startup: bool,
    #[serde(default = "default_jitter_seconds")]
    pub jitter_seconds: u64,
    /// Named schedules. `daily` and `weekly` are built in; one of the same
    /// name here replaces it, and the rest are kept.
    #[serde(
        default = "default_schedules",
        deserialize_with = "schedules_over_defaults"
    )]
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

/// The built-in schedules: `daily` at 03:00, `weekly` at 04:00 on Sundays.
fn default_schedules() -> HashMap<String, ScheduleEntry> {
    let entry = |cron: &str| ScheduleEntry {
        cron: cron.to_string(),
        jitter_seconds: None,
    };
    HashMap::from([
        ("daily".to_string(), entry("0 0 3 * * * *")),
        ("weekly".to_string(), entry("0 0 4 * * 7 *")),
    ])
}

/// The configured schedules over the built-in ones.
fn schedules_over_defaults<'de, D>(d: D) -> Result<HashMap<String, ScheduleEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut schedules = default_schedules();
    schedules.extend(HashMap::<String, ScheduleEntry>::deserialize(d)?);
    Ok(schedules)
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            run_on_startup: false,
            jitter_seconds: 300,
            schedules: default_schedules(),
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
/// Off: a restart (a pod rescheduled, an upgrade) is not a reason to scan
/// every library; scans run on their schedule and when asked.
fn default_run_on_startup() -> bool {
    false
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
        if auth.ldap.is_some() {
            anyhow::bail!(
                "[auth.ldap] is no longer supported: Rustyfin checks no passwords; \
                 sign in through a proxy ([auth.forward_auth]) and remove [auth.ldap]"
            );
        }
        if auth.forward_auth.is_none() {
            anyhow::bail!("[auth.forward_auth] must be configured: it is how people sign in");
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

#[cfg(test)]
mod schedule_tests {
    use super::TasksConfig;

    /// LDAPS sign-in was removed: a config still asking for it is refused at
    /// start rather than quietly signing nobody in.
    #[test]
    fn a_config_with_ldap_is_refused() {
        let auth: super::AuthConfig = config::Config::builder()
            .add_source(config::File::from_str(
                "[ldap]\nurl = \"ldaps://example\"\n[forward_auth]\ntrusted_proxies = []\n",
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        let err = super::Config::validate_auth(&auth).unwrap_err().to_string();
        assert!(err.contains("[auth.ldap] is no longer supported"), "{err}");
    }

    fn tasks(toml: &str) -> TasksConfig {
        config::Config::builder()
            .add_source(config::File::from_str(toml, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap()
    }

    /// The nightly parity run (2026-11-20) would not start: its `[tasks]`
    /// section set only `enabled`, which dropped the built-in schedules the
    /// library scan refers to ("unknown schedule 'daily'").
    #[test]
    fn a_tasks_section_without_schedules_keeps_daily_and_weekly() {
        let t = tasks("enabled = true");
        assert_eq!(t.schedules["daily"].cron, "0 0 3 * * * *");
        assert_eq!(t.schedules["weekly"].cron, "0 0 4 * * 7 *");
    }

    #[test]
    fn a_schedule_of_the_same_name_replaces_the_built_in_one() {
        let t = tasks(
            "[schedules.daily]\ncron = \"0 30 2 * * * *\"\n[schedules.hourly]\ncron = \"0 0 * * * * *\"",
        );
        assert_eq!(t.schedules["daily"].cron, "0 30 2 * * * *");
        assert_eq!(t.schedules["weekly"].cron, "0 0 4 * * 7 *");
        assert_eq!(t.schedules["hourly"].cron, "0 0 * * * * *");
    }
    /// Owner (2026-11-21): "does a restart need to perform a full scan? I
    /// think it should only do scans on the schedule and adhoc. Restarts happen
    /// all the time, even in k8s because the pod gets rescheduled."
    #[test]
    fn a_restart_runs_no_scan_or_task_unless_asked() {
        assert!(!tasks("enabled = true").run_on_startup);
        assert!(!TasksConfig::default().run_on_startup);
        assert!(tasks("run_on_startup = true").run_on_startup);
    }
}
