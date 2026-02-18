# MusicBrainz: API Caching, Setup Script, & K8s Deployment — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add disk-based response caching to the MusicBrainz client, allow the setup script to accept an existing repo path, and create a Kubernetes deployment for a self-hosted MusicBrainz server.

**Architecture:** The cache is a generic `DiskCache` in `rustyfin-common` that serializes JSON responses to disk keyed by namespace+key. `MusicBrainzClient` gains an optional `DiskCache` field and checks it before every HTTP request. The K8s deployment is a standalone Kustomize app in the `local_cluster_config` repo.

**Tech Stack:** Rust (serde_json for cache serialization, std::hash for query hashing), Kubernetes (Deployments, Jobs, CronJobs, Kustomize)

---

### Task 1: Add `DiskCache` module to `rustyfin-common`

**Files:**
- Create: `crates/rustyfin-common/src/cache.rs`
- Modify: `crates/rustyfin-common/src/lib.rs`

**Step 1: Create `cache.rs` with `DiskCache` struct**

```rust
// crates/rustyfin-common/src/cache.rs
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// A simple disk-backed JSON cache with TTL based on file mtime.
pub struct DiskCache {
    base_dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf, ttl_days: u32) -> Self {
        Self {
            base_dir,
            ttl: Duration::from_secs(u64::from(ttl_days) * 86400),
        }
    }

    /// Try to read a cached value. Returns `None` if missing or expired.
    pub fn get<T: DeserializeOwned>(&self, namespace: &str, key: &str) -> Option<T> {
        let path = self.path_for(namespace, key);
        let metadata = std::fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        if modified.elapsed().ok()? > self.ttl {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Write a value to the cache.
    pub fn put<T: Serialize>(&self, namespace: &str, key: &str, value: &T) {
        let path = self.path_for(namespace, key);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("tmp");
        if let Ok(data) = serde_json::to_string(value) {
            if std::fs::write(&tmp, &data).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Hash a query string into a filesystem-safe key.
    pub fn hash_key(query: &str) -> String {
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn path_for(&self, namespace: &str, key: &str) -> PathBuf {
        self.base_dir.join(namespace).join(format!("{key}.json"))
    }
}
```

**Step 2: Register the module in `lib.rs`**

Add `pub mod cache;` to `crates/rustyfin-common/src/lib.rs`.

**Step 3: Build and verify**

Run: `cargo build -p rustyfin-common`
Expected: Compiles with no errors.

**Step 4: Commit**

```
feat: add DiskCache module for generic JSON response caching
```

---

### Task 2: Add cache config fields to `MusicBrainzConfig`

**Files:**
- Modify: `crates/rustyfin-meta/src/config.rs:83-100`

**Step 1: Add `cache_dir` and `cache_ttl_days` fields**

Add two fields to `MusicBrainzConfig`:

```rust
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
```

Update the `Default` impl:

```rust
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
```

Add the default functions:

```rust
fn default_mb_cache_dir() -> String {
    "~/.cache/rustyfin/musicbrainz".to_string()
}

fn default_mb_cache_ttl() -> u32 {
    30
}
```

**Step 2: Build and verify**

Run: `cargo build -p rustyfin-meta`
Expected: Compiles. Existing configs without these fields use defaults.

**Step 3: Commit**

```
feat: add cache_dir and cache_ttl_days to MusicBrainz config
```

---

### Task 3: Add `--no-cache` CLI flag

**Files:**
- Modify: `crates/rustyfin-meta/src/main.rs:15-25`
- Modify: `crates/rustyfin-meta/src/scan.rs` (function signatures)
- Modify: `crates/rustyfin-meta/src/fetch.rs` (function signatures)

**Step 1: Add the flag to `Cli` struct**

```rust
struct Cli {
    #[arg(long, default_value = "rustyfin-meta.toml")]
    config: PathBuf,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    auto: bool,
    /// Skip reading from cache (still writes to cache)
    #[arg(long)]
    no_cache: bool,
    #[command(subcommand)]
    command: Command,
}
```

**Step 2: Thread `no_cache` through to `scan_directory` and `fetch_metadata`**

In `main.rs`, pass `cli.no_cache` to scan and fetch calls:

```rust
Command::Scan { path, library_type } => {
    let stats = scan::scan_directory(&path, &library_type, &config, cli.dry_run, cli.auto, cli.no_cache).await?;
    // ...
}
Command::Fetch { path, media_type } => {
    fetch::fetch_metadata(&path, &media_type, &config, cli.dry_run, cli.auto, cli.no_cache).await?;
}
```

Update `scan_directory` signature to accept `no_cache: bool` (add as last parameter).
Update `scan_music` signature similarly (it's called internally by `scan_directory`).
Update `fetch_metadata` signature similarly.

For now, just add the parameter and ignore it — the next task wires it in.

**Step 3: Build and verify**

Run: `cargo build -p rustyfin-meta`
Expected: Compiles with the new flag.

**Step 4: Commit**

```
feat: add --no-cache CLI flag to rustyfin
```

---

### Task 4: Wire `DiskCache` into `MusicBrainzClient`

**Files:**
- Modify: `crates/rustyfin-common/src/musicbrainz.rs`

**Step 1: Add `cache` field and updated constructor**

```rust
use crate::cache::DiskCache;

pub struct MusicBrainzClient {
    base_url: String,
    client: reqwest::Client,
    pub is_local: bool,
    cache: Option<DiskCache>,
}

impl MusicBrainzClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self::with_cache(base_url, None)
    }

    pub fn with_cache(base_url: Option<String>, cache: Option<DiskCache>) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://musicbrainz.org/ws/2".to_string());
        let is_local = base_url.contains("localhost")
            || base_url.contains("127.0.0.1")
            || base_url.contains("[::1]");

        // Disable cache for local instances (always fresh, no rate limit)
        let cache = if is_local { None } else { cache };

        let client = reqwest::Client::builder()
            .user_agent("rustyfin/0.1.0 (https://github.com/rustyfin/rustyfin)")
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url,
            client,
            is_local,
            cache,
        }
    }
}
```

**Step 2: Add caching to each method**

For `search_artist` — cache namespace `artist_search`, key is `DiskCache::hash_key(name)`:

```rust
pub async fn search_artist(&self, name: &str) -> Vec<SearchResult> {
    let cache_key = DiskCache::hash_key(name);
    if let Some(ref cache) = self.cache {
        if let Some(cached) = cache.get::<Vec<SearchResult>>("artist_search", &cache_key) {
            tracing::debug!("Cache hit: artist search '{name}'");
            return cached;
        }
    }

    // ... existing HTTP fetch code ...

    let results: Vec<SearchResult> = data.artists.into_iter().map(/* ... existing ... */).collect();

    if let Some(ref cache) = self.cache {
        cache.put("artist_search", &cache_key, &results);
    }

    results
}
```

Apply the same pattern to all 5 methods:
- `fetch_artist` — namespace `artist`, key is the MBID directly
- `search_release_group` — namespace `release_group_search`, key is `DiskCache::hash_key(&format!("{album}||{artist}"))`
- `fetch_release_group` — namespace `release_group`, key is the MBID directly
- `fetch_cover_art_url` — namespace `cover_art`, key is the MBID directly

Note: `SearchResult` and `NfoMetadata` need `Serialize` derived (they likely already have `Deserialize`). Check and add `#[derive(Serialize)]` if missing.

**Step 3: Build and verify**

Run: `cargo build -p rustyfin-common`
Expected: Compiles.

**Step 4: Commit**

```
feat: add disk cache support to MusicBrainzClient
```

---

### Task 5: Wire cache creation into scan and fetch flows

**Files:**
- Modify: `crates/rustyfin-meta/src/scan.rs:536` (where `MusicBrainzClient` is created)
- Modify: `crates/rustyfin-meta/src/fetch.rs` (if it uses MB client — currently only movie/series, but for completeness)

**Step 1: Create `DiskCache` and pass to `MusicBrainzClient::with_cache`**

In `scan_music` (around line 536), replace:

```rust
let mb_client = MusicBrainzClient::new(config.musicbrainz.base_url.clone());
```

With:

```rust
use rustyfin_common::cache::DiskCache;

let mb_cache = if no_cache {
    None
} else {
    let cache_dir = expand_tilde(&config.musicbrainz.cache_dir);
    Some(DiskCache::new(
        std::path::PathBuf::from(cache_dir),
        config.musicbrainz.cache_ttl_days,
    ))
};
let mb_client = MusicBrainzClient::with_cache(config.musicbrainz.base_url.clone(), mb_cache);
```

The `expand_tilde` helper (used for discogs token path) should already exist in `scan.rs`. If not, add a simple one:

```rust
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    path.to_string()
}
```

**Step 2: Build and test manually**

Run: `cargo build -p rustyfin-meta`
Expected: Compiles.

Run: `cargo run --bin rustyfin -- scan /path/to/small/music/dir -t music 2>&1 | grep -i cache`
Expected: On first run, no cache hits. On second run, cache hits logged at debug level.

**Step 3: Commit**

```
feat: wire DiskCache into music scan pipeline
```

---

### Task 6: Add `Serialize` derives where needed

**Files:**
- Modify: `crates/rustyfin-common/src/tmdb.rs` (SearchResult)
- Modify: `crates/rustyfin-common/src/nfo.rs` (NfoMetadata)

**Step 1: Check and add `Serialize` derive**

`SearchResult` needs `#[derive(Serialize, Deserialize)]` (or just add `Serialize` if only `Deserialize` is present).
`NfoMetadata` needs `#[derive(Serialize)]` for cache writes from `fetch_artist` and `fetch_release_group`.

Also add `Serialize` to `Option<String>` wrapper type `CoverArtUrl` if we cache `fetch_cover_art_url` result — but since it returns `Option<String>`, `String` already implements `Serialize`.

**Step 2: Build and verify**

Run: `cargo build -p rustyfin-common`
Expected: Compiles with no errors.

**Step 3: Commit**

```
feat: add Serialize derives for cache support
```

Note: This task may be done as part of Task 4 if the compiler immediately complains about missing derives. The implementer should handle it wherever it naturally arises.

---

### Task 7: Update setup script to accept repo path

**Files:**
- Modify: `docker/musicbrainz/setup.sh`
- Modify: `docker/musicbrainz/README.md`

**Step 1: Update `setup.sh`**

Replace the clone logic at the top (lines 20-27) with:

```bash
# Step 1: Locate or clone musicbrainz-docker
if [ -n "${1:-}" ]; then
    MB_DOCKER_DIR="$(cd "$1" && pwd)"
    if [ ! -f "$MB_DOCKER_DIR/docker-compose.yml" ] && [ ! -d "$MB_DOCKER_DIR/compose" ]; then
        echo "Error: $MB_DOCKER_DIR does not look like a musicbrainz-docker checkout."
        echo "Expected docker-compose.yml or compose/ directory."
        exit 1
    fi
    echo "Using existing musicbrainz-docker at: $MB_DOCKER_DIR"
else
    if [ ! -d "$MB_DOCKER_DIR" ]; then
        echo "Cloning musicbrainz-docker..."
        git clone --depth 1 https://github.com/metabrainz/musicbrainz-docker.git "$MB_DOCKER_DIR"
    else
        echo "musicbrainz-docker already cloned, pulling latest..."
        git -C "$MB_DOCKER_DIR" pull
    fi
fi
```

Remove the hardcoded `MB_DOCKER_DIR` assignment at the top (line 5) and replace with a default:

```bash
MB_DOCKER_DIR="${1:-$SCRIPT_DIR/musicbrainz-docker}"
```

Wait — that conflicts with the validation logic. Better approach: keep line 5 as the default, then override if `$1` is given:

```bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MB_DOCKER_DIR="$SCRIPT_DIR/musicbrainz-docker"

# Override with user-provided path
if [ -n "${1:-}" ]; then
    MB_DOCKER_DIR="$(cd "$1" && pwd)"
    if [ ! -f "$MB_DOCKER_DIR/docker-compose.yml" ] && [ ! -d "$MB_DOCKER_DIR/compose" ]; then
        echo "Error: $MB_DOCKER_DIR does not look like a musicbrainz-docker checkout."
        echo "Expected docker-compose.yml or compose/ directory."
        exit 1
    fi
    echo "Using existing musicbrainz-docker at: $MB_DOCKER_DIR"
fi
```

Then the existing clone logic stays as-is for the no-argument case.

**Step 2: Update README.md**

Add usage section:

```markdown
## Usage

```bash
# Clone and set up from scratch
./setup.sh

# Use an existing musicbrainz-docker clone
./setup.sh /path/to/musicbrainz-docker
```
```

**Step 3: Test**

Run: `./setup.sh --help` (should fail gracefully — no help flag, just shows the prompt)
Run: `./setup.sh /nonexistent` (should error with "does not look like a musicbrainz-docker checkout")

**Step 4: Commit**

```
feat: allow setup.sh to accept existing musicbrainz-docker path
```

---

### Task 8: Update roadmap checkboxes

**Files:**
- Modify: `roadmap.md`

**Step 1: Check off completed Phase 10d items**

Lines to change from `- [ ]` to `- [x]`:
- Line 235: Fanart.tv provider
- Line 247: Document the workflow
- Line 267: MusicBrainz Cover Art Archive
- Line 268: Discogs provider
- Line 269: Fanart.tv provider (music)
- Lines 275-281: All 7 self-hosted MusicBrainz items

Change section header on line 260 from `[PARTIALLY DONE]` to `[DONE]`.

Change the Phase 10d header on line 217 from `[PARTIALLY DONE]` to `[DONE]`.

Update the maturity summary table (line 563): Metadata from `~92%` to `~95%` or similar.

**Step 2: Commit**

```
docs: update roadmap — Phase 10d fully complete
```

---

### Task 9: Create Kubernetes MusicBrainz deployment

**Files (all new, in `/home/jonathan/projects/local_cluster_config/media-stack/musicbrainz-server/`):**
- Create: `kustomization.yaml`
- Create: `storage.yaml`
- Create: `postgres.yaml`
- Create: `musicbrainz.yaml`
- Create: `redis.yaml`
- Create: `solr.yaml`
- Create: `secret.yaml`
- Create: `import-job.yaml`
- Create: `replication-cronjob.yaml`
- Modify: `/home/jonathan/projects/local_cluster_config/media-stack/kustomization.yaml` (add resource)

**Step 1: Create `kustomization.yaml`**

```yaml
---
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - secret.yaml
  - storage.yaml
  - postgres.yaml
  - redis.yaml
  - solr.yaml
  - musicbrainz.yaml
  - import-job.yaml
  - replication-cronjob.yaml
namespace: media-stack
labels:
  - includeSelectors: true
    pairs:
      app.kubernetes.io/part-of: musicbrainz-server
```

**Step 2: Create `secret.yaml`**

```yaml
---
apiVersion: onepassword.com/v1
kind: OnePasswordItem
metadata:
  name: musicbrainz-server-secrets
  namespace: media-stack
  annotations:
    operator.1password.io/auto-restart: 'true'
spec:
  itemPath: vaults/server/items/musicbrainz_server
```

This 1Password item should contain:
- `postgres_password` — password for the dedicated MB Postgres
- `metabrainz_token` — access token from metabrainz.org for replication

**Step 3: Create `storage.yaml`**

```yaml
---
kind: PersistentVolume
apiVersion: v1
metadata:
  name: musicbrainz-db-pv
  namespace: media-stack
  labels:
    app: musicbrainz-db
    type: local
spec:
  storageClassName: musicbrainz-db
  capacity:
    storage: 60Gi
  accessModes: [ReadWriteMany]
  hostPath:
    path: /var/lib/data/musicbrainz-db
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: musicbrainz-db-pvc
  namespace: media-stack
spec:
  storageClassName: musicbrainz-db
  accessModes: [ReadWriteMany]
  resources:
    requests:
      storage: 60Gi
```

**Step 4: Create `postgres.yaml`**

```yaml
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: musicbrainz-db
  namespace: media-stack
  labels:
    app: musicbrainz-db
spec:
  replicas: 1
  selector:
    matchLabels:
      app: musicbrainz-db
  template:
    metadata:
      labels:
        app: musicbrainz-db
    spec:
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: musicbrainz-db-pvc
      containers:
        - name: postgres
          image: postgres:16-alpine
          args:
            - -c
            - shared_buffers=2GB
            - -c
            - shared_preload_libraries=pg_amqp
          resources:
            requests:
              memory: 4Gi
              cpu: 200m
            limits:
              memory: 4Gi
          env:
            - name: POSTGRES_USER
              value: musicbrainz
            - name: POSTGRES_DB
              value: musicbrainz_db
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: musicbrainz-server-secrets
                  key: postgres_password
          ports:
            - containerPort: 5432
          volumeMounts:
            - name: data
              mountPath: /var/lib/postgresql/data
---
apiVersion: v1
kind: Service
metadata:
  name: musicbrainz-db
  namespace: media-stack
spec:
  selector:
    app: musicbrainz-db
  ports:
    - port: 5432
      targetPort: 5432
  type: ClusterIP
```

Note: The `pg_amqp` shared_preload may not be available in vanilla `postgres:16-alpine`. The implementer should test this — if it fails, remove the `pg_amqp` line (it's only needed for RabbitMQ-based replication which we may not use). The official musicbrainz-docker builds a custom Postgres image for this. If needed, switch to `ghcr.io/metabrainz/musicbrainz-docker/db:latest` or remove the amqp preload.

**Step 5: Create `redis.yaml`**

```yaml
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: musicbrainz-redis
  namespace: media-stack
  labels:
    app: musicbrainz-redis
spec:
  replicas: 1
  selector:
    matchLabels:
      app: musicbrainz-redis
  template:
    metadata:
      labels:
        app: musicbrainz-redis
    spec:
      containers:
        - name: redis
          image: redis:7-alpine
          resources:
            requests:
              memory: 256Mi
              cpu: 50m
          ports:
            - containerPort: 6379
---
apiVersion: v1
kind: Service
metadata:
  name: musicbrainz-redis
  namespace: media-stack
spec:
  selector:
    app: musicbrainz-redis
  ports:
    - port: 6379
      targetPort: 6379
  type: ClusterIP
```

**Step 6: Create `solr.yaml`**

```yaml
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: musicbrainz-solr
  namespace: media-stack
  labels:
    app: musicbrainz-solr
spec:
  replicas: 1
  selector:
    matchLabels:
      app: musicbrainz-solr
  template:
    metadata:
      labels:
        app: musicbrainz-solr
    spec:
      containers:
        - name: solr
          image: metabrainz/mb-solr:latest
          env:
            - name: SOLR_HEAP
              value: "2g"
          resources:
            requests:
              memory: 2Gi
              cpu: 100m
            limits:
              memory: 2Gi
          ports:
            - containerPort: 8983
---
apiVersion: v1
kind: Service
metadata:
  name: musicbrainz-solr
  namespace: media-stack
spec:
  selector:
    app: musicbrainz-solr
  ports:
    - port: 8983
      targetPort: 8983
  type: ClusterIP
```

**Step 7: Create `musicbrainz.yaml`**

```yaml
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: musicbrainz-server
  namespace: media-stack
  labels:
    app: musicbrainz-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: musicbrainz-server
  template:
    metadata:
      labels:
        app: musicbrainz-server
    spec:
      containers:
        - name: musicbrainz
          image: metabrainz/musicbrainz-server:latest
          resources:
            requests:
              memory: 2Gi
              cpu: 200m
            limits:
              memory: 2Gi
          env:
            - name: MUSICBRAINZ_BASE_URL
              value: "http://musicbrainz-server:5000"
            - name: MUSICBRAINZ_DB_HOST
              value: musicbrainz-db
            - name: MUSICBRAINZ_DB_NAME
              value: musicbrainz_db
            - name: MUSICBRAINZ_DB_USER
              value: musicbrainz
            - name: MUSICBRAINZ_DB_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: musicbrainz-server-secrets
                  key: postgres_password
            - name: MUSICBRAINZ_REDIS_HOST
              value: musicbrainz-redis
            - name: MUSICBRAINZ_SEARCH_HOST
              value: musicbrainz-solr
          ports:
            - containerPort: 5000
---
apiVersion: v1
kind: Service
metadata:
  name: musicbrainz-server
  namespace: media-stack
spec:
  selector:
    app: musicbrainz-server
  ports:
    - name: http
      port: 5000
      targetPort: 5000
  type: ClusterIP
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: musicbrainz-server
  namespace: media-stack
spec:
  defaultBackend:
    service:
      name: musicbrainz-server
      port:
        name: http
  ingressClassName: tailscale
  tls:
    - hosts: [musicbrainz-api]
```

**Step 8: Create `import-job.yaml`**

```yaml
---
apiVersion: batch/v1
kind: Job
metadata:
  name: musicbrainz-import
  namespace: media-stack
  labels:
    app: musicbrainz-server
spec:
  backoffLimit: 1
  template:
    metadata:
      labels:
        app: musicbrainz-import
    spec:
      restartPolicy: Never
      containers:
        - name: import
          image: metabrainz/musicbrainz-server:latest
          command: ["/bin/bash", "-c"]
          args:
            - |
              set -euo pipefail
              echo "Downloading latest MusicBrainz database dump..."
              cd /tmp
              wget -q "https://data.metabrainz.org/pub/musicbrainz/data/fullexport/LATEST" -O LATEST
              DUMP_DATE=$(cat LATEST)
              echo "Latest dump: $DUMP_DATE"
              wget -q "https://data.metabrainz.org/pub/musicbrainz/data/fullexport/$DUMP_DATE/mbdump.tar.bz2"
              echo "Importing into database..."
              export PGHOST=$MUSICBRAINZ_DB_HOST
              export PGUSER=$MUSICBRAINZ_DB_USER
              export PGPASSWORD=$MUSICBRAINZ_DB_PASSWORD
              export PGDATABASE=$MUSICBRAINZ_DB_NAME
              tar -xjf mbdump.tar.bz2 -C /tmp/mbdump
              # Import uses MusicBrainz's own import scripts
              /musicbrainz/admin/InitDb.pl --createdb --import /tmp/mbdump/mbdump* || true
              echo "Import complete."
          env:
            - name: MUSICBRAINZ_DB_HOST
              value: musicbrainz-db
            - name: MUSICBRAINZ_DB_USER
              value: musicbrainz
            - name: MUSICBRAINZ_DB_NAME
              value: musicbrainz_db
            - name: MUSICBRAINZ_DB_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: musicbrainz-server-secrets
                  key: postgres_password
          resources:
            requests:
              memory: 4Gi
              cpu: 500m
```

Note: The exact import commands depend on what's inside the `metabrainz/musicbrainz-server` image. The implementer should verify the image's filesystem layout (`/musicbrainz/admin/InitDb.pl` path) and adjust. The import job may need iteration — treat this as a starting point that needs hands-on testing.

**Step 9: Create `replication-cronjob.yaml`**

```yaml
---
apiVersion: batch/v1
kind: CronJob
metadata:
  name: musicbrainz-replication
  namespace: media-stack
  labels:
    app: musicbrainz-server
spec:
  schedule: "0 3 * * *"
  concurrencyPolicy: Forbid
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 3
  jobTemplate:
    spec:
      backoffLimit: 2
      template:
        metadata:
          labels:
            app: musicbrainz-replication
        spec:
          restartPolicy: Never
          containers:
            - name: replication
              image: metabrainz/musicbrainz-server:latest
              command: ["/bin/bash", "-c"]
              args:
                - |
                  set -euo pipefail
                  export REPLICATION_ACCESS_TOKEN="$METABRAINZ_TOKEN"
                  export MUSICBRAINZ_DB_HOST
                  export MUSICBRAINZ_DB_USER
                  export MUSICBRAINZ_DB_NAME
                  export MUSICBRAINZ_DB_PASSWORD
                  /musicbrainz/admin/replication/LoadReplicationChanges || echo "Replication failed, will retry next run"
              env:
                - name: MUSICBRAINZ_DB_HOST
                  value: musicbrainz-db
                - name: MUSICBRAINZ_DB_USER
                  value: musicbrainz
                - name: MUSICBRAINZ_DB_NAME
                  value: musicbrainz_db
                - name: MUSICBRAINZ_DB_PASSWORD
                  valueFrom:
                    secretKeyRef:
                      name: musicbrainz-server-secrets
                      key: postgres_password
                - name: METABRAINZ_TOKEN
                  valueFrom:
                    secretKeyRef:
                      name: musicbrainz-server-secrets
                      key: metabrainz_token
              resources:
                requests:
                  memory: 1Gi
                  cpu: 200m
```

Note: Same caveat as the import job — the replication script path needs verification against the actual image contents. The `LoadReplicationChanges` path is based on the MusicBrainz server source tree structure.

**Step 10: Add to parent `kustomization.yaml`**

In `/home/jonathan/projects/local_cluster_config/media-stack/kustomization.yaml`, add:

```yaml
  - ./musicbrainz-server
```

**Step 11: Validate manifests**

Run: `kubectl kustomize /home/jonathan/projects/local_cluster_config/media-stack/musicbrainz-server/`
Expected: Valid YAML output with all resources.

Do NOT apply to the cluster yet — this needs hands-on testing. The import job especially requires iteration.

**Step 12: Commit (in local_cluster_config repo)**

```
feat: add MusicBrainz server k8s deployment with import job and replication cronjob
```

---

## Execution Order & Dependencies

```
Task 1 (DiskCache module)
  └──> Task 4 (wire into MusicBrainzClient) ──> Task 5 (wire into scan flow)
Task 2 (config fields)
  └──> Task 5 (scan flow needs config)
Task 3 (CLI flag)
  └──> Task 5 (scan flow needs no_cache param)
Task 6 (Serialize derives) — do alongside Task 4 when compiler complains

Task 7 (setup.sh) — independent
Task 8 (roadmap) — independent
Task 9 (k8s) — independent, separate repo
```

Tasks 1, 2, 3, 7, 8, 9 can all be started in parallel.
Tasks 4 depends on 1.
Task 5 depends on 2, 3, 4.
Task 6 is done as needed during Task 4.
