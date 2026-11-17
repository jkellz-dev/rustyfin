//! `rustyfin-bench`: the numbers in `docs/performance.md`, reproducible.
//!
//! Generates a library with `tests/stress/make_library.sh`, starts the release
//! server against a fresh database, times the first scan and a rescan, runs the
//! browsing queries with a fixed number of concurrent clients, and records the
//! server's memory and the machine it ran on. Prints a Markdown report and
//! writes the same results as TOML.
//!
//! ```text
//! mise run bench                     # the full library (81,390 files)
//! mise run bench -- --scale 0.1      # a tenth of it, a few minutes
//! ```
//!
//! Needs a Postgres it may create databases in (`DATABASE_URL`), `ffprobe` on
//! PATH (the scan probes every new file) and a release build of the server.

mod machine;
mod report;
mod stats;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use reqwest::Url;
use serde_json::Value;
use sqlx::Connection;
use tokio::process::{Child, Command};

use crate::report::{Browse, Results, Scan};

#[derive(Parser, Debug)]
#[command(about = "Scan and browse a generated library; report the timings")]
struct Args {
    /// Library size: 1.0 is 2,700 films, 300 shows and 1,500 artists
    /// (81,390 files); 0.1 is a tenth of each.
    #[arg(long, default_value_t = 1.0)]
    scale: f64,
    /// Concurrent clients per browsing query.
    #[arg(long, default_value_t = 5)]
    clients: usize,
    /// Seconds each browsing query runs.
    #[arg(long, default_value_t = 30)]
    seconds: u64,
    /// Postgres to create the benchmark database in (dropped afterwards).
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    /// The server binary (default: target/release/rustyfin-server).
    #[arg(long)]
    server: Option<PathBuf>,
    /// Where the library, logs and server state go (default: a new folder in
    /// the system temp dir, removed afterwards unless --keep).
    #[arg(long)]
    work_dir: Option<PathBuf>,
    /// Results file (TOML).
    #[arg(long, default_value = "bench-results.toml")]
    out: PathBuf,
    /// Keep the work folder and the database.
    #[arg(long)]
    keep: bool,
}

/// The repository root (this crate sits in `crates/rustyfin-bench`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.scale.is_nan() || args.scale <= 0.0 {
        bail!("--scale must be above 0");
    }
    let root = repo_root();
    let server_bin = args
        .server
        .clone()
        .unwrap_or_else(|| root.join("target/release/rustyfin-server"));
    if !server_bin.exists() {
        bail!(
            "no server binary at {}: build it with `cargo build --release -p rustyfin-server`",
            server_bin.display()
        );
    }
    let ffprobe = std::process::Command::new("ffprobe")
        .arg("-version")
        .output();
    if !ffprobe.is_ok_and(|o| o.status.success()) {
        bail!("ffprobe is not on PATH; the scan probes every new file with it");
    }

    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    let work = args
        .work_dir
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("rustyfin-bench-{stamp}")));
    std::fs::create_dir_all(&work)?;
    let db_name = format!("rustyfin_bench_{stamp}");

    let run = Run {
        args: &args,
        root: &root,
        work: &work,
        db_name: &db_name,
    };
    let outcome = run.go(&server_bin).await;

    if !args.keep {
        if let Err(e) = drop_database(&args.database_url, &db_name).await {
            eprintln!("could not drop {db_name}: {e:#}");
        }
        let _ = std::fs::remove_dir_all(&work);
    } else {
        eprintln!("kept {} and database {db_name}", work.display());
    }

    let results = outcome?;
    std::fs::write(&args.out, toml::to_string(&results)?)?;
    print!("{}", results.markdown());
    eprintln!("results written to {}", args.out.display());
    Ok(())
}

struct Run<'a> {
    args: &'a Args,
    root: &'a Path,
    work: &'a Path,
    db_name: &'a str,
}

impl Run<'_> {
    async fn go(&self, server_bin: &Path) -> anyhow::Result<Results> {
        let media = self.work.join("media");
        let counts = generate_library(self.root, &media, self.args.scale)?;
        eprintln!("library: {} files in {}", counts.files, media.display());

        create_database(&self.args.database_url, self.db_name).await?;
        let port = free_port()?;
        let mut server = self.start_server(server_bin, port).await?;
        let pid = server.id().context("server pid")?;
        let api = Api::sign_in(port).await?;

        for (name, kind, folder) in [
            ("Movies", "movies", "movies"),
            ("Shows", "tvshows", "shows"),
            ("Music", "music", "music"),
        ] {
            api.add_library(name, kind, &media.join(folder)).await?;
        }

        eprintln!("first scan ...");
        let cpu0 = machine::cpu_seconds(pid);
        let first = api.scan().await?;
        let cpu1 = machine::cpu_seconds(pid);
        let first_scan = Scan {
            seconds: first.as_secs_f64(),
            server_cores: cores(cpu0, cpu1, first),
            peak_memory_mib: machine::peak_memory_mib(pid),
            items: api.counts().await?,
            errors: api.scan_errors().await?,
        };

        eprintln!("rescan ...");
        let cpu0 = machine::cpu_seconds(pid);
        let again = api.scan().await?;
        let cpu1 = machine::cpu_seconds(pid);
        let rescan = Scan {
            seconds: again.as_secs_f64(),
            server_cores: cores(cpu0, cpu1, again),
            peak_memory_mib: machine::peak_memory_mib(pid),
            items: api.counts().await?,
            errors: api.scan_errors().await?,
        };

        let idle_memory_mib = machine::memory_mib(pid);
        let mut browse = Vec::new();
        for (name, query) in browse_queries(&first_scan.items) {
            eprintln!("browse: {name} ...");
            let cpu0 = machine::cpu_seconds(pid);
            let r = api
                .load(
                    &query,
                    self.args.clients,
                    Duration::from_secs(self.args.seconds),
                )
                .await;
            let cpu1 = machine::cpu_seconds(pid);
            browse.push(Browse {
                query: name,
                url: query,
                clients: self.args.clients,
                requests: r.latencies_ms.len(),
                errors: r.errors,
                per_second: r.latencies_ms.len() as f64 / r.elapsed.as_secs_f64(),
                p50_ms: stats::percentile(&r.latencies_ms, 50.0),
                p95_ms: stats::percentile(&r.latencies_ms, 95.0),
                p99_ms: stats::percentile(&r.latencies_ms, 99.0),
                server_cores: cores(cpu0, cpu1, r.elapsed),
            });
        }
        let browse_memory_mib = machine::memory_mib(pid);
        let peak_memory_mib = machine::peak_memory_mib(pid);
        let _ = server.kill().await;

        Ok(Results {
            date: chrono::Utc::now().to_rfc3339(),
            commit: machine::git_commit(self.root),
            scale: self.args.scale,
            files: counts.files,
            machine: machine::describe(),
            first_scan,
            rescan,
            browse,
            idle_memory_mib,
            browse_memory_mib,
            peak_memory_mib,
        })
    }

    /// The server in its own folder (a stray `rustyfin.toml` in the current
    /// directory must not reach it), with the repository's migrations.
    async fn start_server(&self, bin: &Path, port: u16) -> anyhow::Result<Child> {
        let dir = self.work.join("server");
        std::fs::create_dir_all(&dir)?;
        let migrations = dir.join("migrations");
        if !migrations.exists() {
            std::os::unix::fs::symlink(self.root.join("migrations"), &migrations)?;
        }
        let db = Url::parse(&self.args.database_url).context("DATABASE_URL")?;
        let config = serde_json::json!({
            "server": {
                "port": port,
                "base_url": format!("http://127.0.0.1:{port}"),
                "image_cache_dir": dir.join("images"),
            },
            "database": {
                "host": db.host_str().unwrap_or("localhost"),
                "port": db.port().unwrap_or(5432),
                "user": db.username(),
                "password": db.password().unwrap_or(""),
                "name": self.db_name,
            },
            "transcode": { "temp_dir": dir.join("transcode") },
            "auth": { "forward_auth": {
                "trusted_proxies": ["127.0.0.1/32", "::1/128"],
                "admin_group": "admin",
            }},
        });
        let config_path = dir.join("rustyfin.toml");
        std::fs::write(&config_path, toml::to_string(&config)?)?;
        let log = std::fs::File::create(dir.join("server.log"))?;

        let child = Command::new(bin)
            .current_dir(&dir)
            .env("RUSTYFIN_CONFIG", &config_path)
            .env("RUST_LOG", "warn,rustyfin_core::library=info")
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .context("start the server")?;

        let ping = format!("http://127.0.0.1:{port}/System/Ping");
        for _ in 0..120 {
            if reqwest::get(&ping)
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                return Ok(child);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        bail!(
            "the server did not answer within 60 s; see {}",
            dir.join("server.log").display()
        )
    }
}

fn cores(cpu0: Option<f64>, cpu1: Option<f64>, over: Duration) -> Option<f64> {
    Some((cpu1? - cpu0?) / over.as_secs_f64())
}

struct LibraryCounts {
    files: u64,
}

/// Run `tests/stress/make_library.sh` with the counts scaled.
fn generate_library(root: &Path, out: &Path, scale: f64) -> anyhow::Result<LibraryCounts> {
    let scaled = |n: f64| ((n * scale).round() as u64).max(1).to_string();
    let output = std::process::Command::new("sh")
        .arg(root.join("tests/stress/make_library.sh"))
        .arg(out)
        .env("FILMS", scaled(2700.0))
        .env("SHOWS", scaled(300.0))
        .env("ARTISTS", scaled(1500.0))
        .output()
        .context("run make_library.sh")?;
    if !output.status.success() {
        bail!(
            "make_library.sh failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let files = text
        .lines()
        .find_map(|l| l.strip_prefix("files: "))
        .and_then(|n| n.trim().parse().ok())
        .context("make_library.sh did not report its file count")?;
    Ok(LibraryCounts { files })
}

/// The browsing queries, named by the size of the set they page through.
fn browse_queries(items: &report::Items) -> Vec<(String, String)> {
    let page = "Recursive=true&SortBy=SortName&Limit=100";
    let late = items.tracks.saturating_sub(items.tracks / 10);
    vec![
        (
            format!("{} shows, first page", items.shows),
            format!("/Items?IncludeItemTypes=Series&{page}"),
        ),
        (
            format!("{} films, first page", items.films),
            format!("/Items?IncludeItemTypes=Movie&{page}"),
        ),
        (
            format!("{} albums, first page", items.albums),
            format!("/Items?IncludeItemTypes=MusicAlbum&{page}"),
        ),
        (
            format!("{} tracks, first page", items.tracks),
            format!("/Items?IncludeItemTypes=Audio&{page}"),
        ),
        (
            format!("{} tracks, page at {late}", items.tracks),
            format!("/Items?IncludeItemTypes=Audio&{page}&StartIndex={late}"),
        ),
        (
            format!("every item ({}), first page", items.total),
            format!("/Items?{page}"),
        ),
        (
            "name search over every item".to_string(),
            "/Items?Recursive=true&SearchTerm=track%2042&Limit=50".to_string(),
        ),
    ]
}

fn free_port() -> anyhow::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// Database names here are `rustyfin_bench_<digits>`, never user input.
async fn create_database(admin_url: &str, name: &str) -> anyhow::Result<()> {
    let mut conn = sqlx::PgConnection::connect(admin_url)
        .await
        .context("connect to DATABASE_URL")?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut conn)
        .await
        .context("create the benchmark database (the role needs CREATEDB)")?;
    Ok(())
}

async fn drop_database(admin_url: &str, name: &str) -> anyhow::Result<()> {
    let mut conn = sqlx::PgConnection::connect(admin_url).await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
    )))
    .execute(&mut conn)
    .await?;
    Ok(())
}

/// A signed-in admin session, the way a forward-auth proxy would sign one in.
struct Api {
    http: reqwest::Client,
    base: String,
    auth: String,
}

struct Load {
    latencies_ms: Vec<f64>,
    errors: usize,
    elapsed: Duration,
}

impl Api {
    async fn sign_in(port: u16) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        let base = format!("http://127.0.0.1:{port}");
        let client = r#"MediaBrowser Client="bench", Device="bench", DeviceId="rustyfin-bench", Version="1""#;
        let r: Value = http
            .post(format!("{base}/Users/AuthenticateByName"))
            .header("Authorization", client)
            .header("Remote-User", "bench")
            .header("Remote-Groups", "admin")
            .json(&serde_json::json!({ "Username": "bench", "Pw": "" }))
            .send()
            .await?
            .error_for_status()
            .context("sign in")?
            .json()
            .await?;
        let token = r["AccessToken"].as_str().context("no AccessToken")?;
        Ok(Self {
            http,
            base,
            auth: format!(r#"{client}, Token="{token}""#),
        })
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.base))
            .header("Authorization", &self.auth)
    }

    async fn add_library(&self, name: &str, kind: &str, folder: &Path) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/Library/VirtualFolders", self.base))
            .header("Authorization", &self.auth)
            .query(&[
                ("name", name),
                ("collectionType", kind),
                ("refreshLibrary", "false"),
                ("paths", &folder.display().to_string()),
            ])
            .json(&serde_json::json!({ "LibraryOptions": {} }))
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("add library {name}"))?;
        Ok(())
    }

    async fn folders(&self) -> anyhow::Result<Vec<Value>> {
        Ok(self
            .get("/Library/VirtualFolders")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Scan every library; returns once all are idle again.
    async fn scan(&self) -> anyhow::Result<Duration> {
        let started = Instant::now();
        self.http
            .post(format!("{}/Library/Refresh", self.base))
            .header("Authorization", &self.auth)
            .send()
            .await?
            .error_for_status()?;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let folders = self.folders().await?;
            if folders.iter().all(|f| f["RefreshStatus"] == "Idle") {
                return Ok(started.elapsed());
            }
        }
    }

    async fn scan_errors(&self) -> anyhow::Result<u64> {
        Ok(self
            .folders()
            .await?
            .iter()
            .filter_map(|f| f["LastScanErrors"].as_u64())
            .sum())
    }

    async fn count(&self, filter: &str) -> anyhow::Result<u64> {
        let r: Value = self
            .get(&format!("/Items?Recursive=true&Limit=0{filter}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        r["TotalRecordCount"].as_u64().context("TotalRecordCount")
    }

    async fn counts(&self) -> anyhow::Result<report::Items> {
        Ok(report::Items {
            films: self.count("&IncludeItemTypes=Movie").await?,
            shows: self.count("&IncludeItemTypes=Series").await?,
            episodes: self.count("&IncludeItemTypes=Episode").await?,
            albums: self.count("&IncludeItemTypes=MusicAlbum").await?,
            tracks: self.count("&IncludeItemTypes=Audio").await?,
            total: self.count("").await?,
        })
    }

    /// `clients` loops requesting `path` for `length`; every answer's time.
    async fn load(&self, path: &str, clients: usize, length: Duration) -> Load {
        let started = Instant::now();
        let mut tasks = Vec::new();
        for _ in 0..clients {
            let req = self.get(path);
            tasks.push(tokio::spawn(async move {
                let mut times = Vec::new();
                let mut errors = 0;
                while started.elapsed() < length {
                    let Some(r) = req.try_clone() else { break };
                    let t = Instant::now();
                    match r.send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let _ = resp.bytes().await;
                            times.push(t.elapsed().as_secs_f64() * 1000.0);
                        }
                        _ => errors += 1,
                    }
                }
                (times, errors)
            }));
        }
        let mut latencies_ms = Vec::new();
        let mut errors = 0;
        for task in tasks {
            if let Ok((times, e)) = task.await {
                latencies_ms.extend(times);
                errors += e;
            }
        }
        Load {
            latencies_ms,
            errors,
            elapsed: started.elapsed(),
        }
    }
}
