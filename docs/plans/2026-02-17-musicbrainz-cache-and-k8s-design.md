# MusicBrainz: API Caching, Setup Script, & Kubernetes Deployment

**Date**: 2026-02-17

## Context

The `rustyfin` CLI queries the public MusicBrainz API at 1 req/sec. For large music libraries, repeat scans re-fetch the same data from the API even though MusicBrainz metadata rarely changes. A disk cache eliminates redundant requests and makes repeat scans nearly instant.

Separately, the self-hosted MusicBrainz Docker setup always clones the official `musicbrainz-docker` repo. Users who already have a clone shouldn't need to re-clone.

Finally, a personal Kubernetes deployment of MusicBrainz server replaces the Docker Compose setup for cluster environments. This lives in the `local_cluster_config` repo, not in Rustyfin itself.

---

## Piece 1: Setup Script Accepts Existing Repo Path

**File**: `docker/musicbrainz/setup.sh`

### Changes

- Accept optional `$1` as path to an existing `musicbrainz-docker` clone
- Validate the path contains expected markers (`docker-compose.yml` or `compose/` directory)
- If no argument provided, clone as before (backward compatible)
- Update `docker/musicbrainz/README.md` to document the argument

### Usage

```bash
./setup.sh                                      # clones fresh (current behavior)
./setup.sh /path/to/existing/musicbrainz-docker  # uses existing clone
```

---

## Piece 2: MusicBrainz API Disk Cache

### New Module: `crates/rustyfin-common/src/cache.rs`

Generic disk cache struct reusable by any provider:

```rust
pub struct DiskCache {
    base_dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf, ttl_days: u32) -> Self;
    pub fn get<T: DeserializeOwned>(&self, namespace: &str, key: &str) -> Option<T>;
    pub fn put<T: Serialize>(&self, namespace: &str, key: &str, value: &T);
}
```

- **TTL check**: file mtime vs configured duration. Expired entries are treated as misses.
- **Atomic writes**: write to `{path}.tmp`, then `fs::rename` to final path.
- **Directory creation**: `create_dir_all` on first write per namespace.
- **Serialization**: `serde_json` (human-readable, debuggable).

### Cache Directory Layout

```
~/.cache/rustyfin/musicbrainz/
├── artist_search/{sha256_of_query}.json
├── artist/{mbid}.json
├── release_group_search/{sha256_of_query}.json
├── release_group/{mbid}.json
└── cover_art/{mbid}.json
```

- Entity fetches keyed by MBID (human-readable, collision-free)
- Search queries keyed by SHA-256 of the query string

### Config Additions

`[musicbrainz]` in `rustyfin-meta.toml`:

```toml
cache_dir = "~/.cache/rustyfin/musicbrainz"   # default
cache_ttl_days = 30                            # default
```

New fields on `MusicBrainzConfig`:

```rust
pub struct MusicBrainzConfig {
    pub base_url: Option<String>,
    pub rate_limit: u32,
    pub rate_window_secs: u64,
    pub cache_dir: Option<String>,       // NEW — default ~/.cache/rustyfin/musicbrainz
    pub cache_ttl_days: Option<u32>,     // NEW — default 30
}
```

### Changes to `MusicBrainzClient`

- Add `cache: Option<DiskCache>` field
- `MusicBrainzClient::new()` accepts optional cache config
- Each method: check cache → return on hit → else HTTP fetch → write to cache → return
- Cache disabled when `is_local` is true (local instance has no rate limit, data is always fresh)

### CLI Flag

`--no-cache` on `rustyfin fetch` and `rustyfin scan`:
- Bypasses cache reads (always fetches fresh)
- Still writes responses to cache (primes the cache for next run)

### What Stays the Same

- All MusicBrainz API response types unchanged
- Rate limiting still applies on cache misses
- `NfoMetadata` merge logic unchanged
- Scan flow structure unchanged — cache is transparent at the client level

---

## Piece 3: Kubernetes MusicBrainz Deployment (Personal)

**Location**: `/home/jonathan/projects/local_cluster_config/media-stack/musicbrainz-server/`

This is separate from the Rustyfin project. It deploys the MusicBrainz server API for unlimited local lookups in a Kubernetes cluster.

### Architecture

```
┌─────────────────────────────────────────────┐
│  musicbrainz-server/ (Kustomize)            │
│                                             │
│  Deployments:                               │
│  ├── musicbrainz-db    (postgres:16)        │
│  ├── musicbrainz       (metabrainz/...      │
│  │                      musicbrainz-server) │
│  ├── musicbrainz-redis (redis:7-alpine)     │
│  └── musicbrainz-solr  (metabrainz/mb-solr) │
│                                             │
│  Job:                                       │
│  └── musicbrainz-import (one-time dump)     │
│                                             │
│  CronJob:                                   │
│  └── musicbrainz-replication (daily ~3 AM)  │
│                                             │
│  Ingress:                                   │
│  └── tailscale (musicbrainz-api)            │
└─────────────────────────────────────────────┘
```

### Manifests

| File | Purpose |
|------|---------|
| `kustomization.yaml` | Resource list, namespace, labels |
| `storage.yaml` | Dedicated PV/PVC: `musicbrainz-db-pv` (hostPath `/var/lib/data/musicbrainz-db`, 60Gi) |
| `postgres.yaml` | Postgres 16 Deployment + ClusterIP Service (`musicbrainz-db:5432`) |
| `deployment.yaml` | MusicBrainz server Deployment (port 5000) |
| `redis.yaml` | Redis 7 Deployment + ClusterIP Service |
| `solr.yaml` | Solr Deployment + ClusterIP Service |
| `service.yaml` | MusicBrainz server ClusterIP Service |
| `ingress.yaml` | Tailscale ingress for API access |
| `secrets.yaml` | OnePasswordItem for MetaBrainz access token + Postgres credentials |
| `import-job.yaml` | One-time Job: download full dump (~30GB), pg_restore into dedicated Postgres |
| `replication-cronjob.yaml` | Daily CronJob: download incremental packet, apply to Postgres |

### Patterns (matching existing cluster conventions)

- **Namespace**: `media-stack`
- **Storage**: Manual hostPath PV/PVC, custom StorageClass `musicbrainz-db`
- **Ingress**: `ingressClassName: tailscale`
- **Resources**: Postgres 4Gi memory limit, MusicBrainz server 2Gi, Redis/Solr 512Mi
- **Security**: `fsGroup: 1000`
- **Secrets**: OnePasswordItem CRD

### Import Job

Runs once after initial deploy:
1. Download latest full database dump from `data.metabrainz.org`
2. Extract and `pg_restore` into dedicated Postgres
3. Build Solr search indexes

### Replication CronJob

Runs daily at 3 AM UTC:
1. Download incremental replication packet (few MB)
2. Apply to Postgres via `replication.sh` from the MusicBrainz image
3. Requires MetaBrainz access token (free registration)

### Integration with `media-stack/kustomization.yaml`

Add resource entry:
```yaml
- ./musicbrainz-server
```

---

## Roadmap Updates

Check off completed Phase 10d items:
- [x] Fanart.tv provider
- [x] MusicBrainz Cover Art Archive
- [x] Discogs provider
- [x] Fanart.tv music provider
- [x] Self-hosted MusicBrainz (all 7 sub-items)
- [x] Workflow documentation
- Section header: "PARTIALLY DONE" → "DONE"
