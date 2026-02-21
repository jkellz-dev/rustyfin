# CI Pipeline Design

## Goal

GitHub Actions CI/CD pipeline that gates PRs with lint+test, and on main push: builds multi-arch binaries, pushes two container images to GHCR, and creates a GitHub Release with git-cliff release notes.

## Triggers

- **`pull_request`**: lint, unit tests, integration tests (gate PRs)
- **`push` to `main`**: all of the above, plus build, container push, GitHub Release

Concurrency: cancel in-progress runs for the same PR/branch.

## Workflow: `.github/workflows/ci.yml`

Single workflow file with parallel jobs and dependency edges.

### Job Dependency Graph

```
PR triggers:
  lint ─────────────────┐
  test-unit ────────────┤ (parallel)
  test-integration ─────┘

Main push (in addition):
  build-frontend ───┐
                    ├──→ build ──→ container-push ──→ release
  (lint+tests pass) ┘                                   ↑
  changelog (git-cliff) ────────────────────────────────┘
```

### Job Details

**lint** (`ubuntu-latest`)
- Checkout → rust toolchain (stable) → `Swatinem/rust-cache`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`

**test-unit** (`ubuntu-latest`)
- Checkout → rust toolchain → rust-cache
- `cargo test --lib`

**test-integration** (`ubuntu-latest`)
- Postgres 17 service container
- `DATABASE_URL` env var pointing to service container
- Checkout → rust toolchain → rust-cache
- `cargo test -p rustyfin-api --test '*' -- --test-threads=1`

**build-frontend** (`ubuntu-latest`)
- Install `trunk` CLI + `wasm32-unknown-unknown` target
- `trunk build --release` in `frontend/`
- Upload `frontend/dist/` as workflow artifact

**build** (depends on: build-frontend, lint, test-unit, test-integration; main only)
- Download frontend artifact into `frontend/dist/`
- Cross-compile for linux/amd64 and linux/arm64 using `cross`
- Produces 4 binaries: `rustyfin-server` and `rustyfin` for each arch
- Upload as workflow artifacts

**container-push** (depends on: build; main only)
- `docker/build-push-action` with buildx for multi-arch images
- Two images pushed to GHCR (private):
  - `ghcr.io/jkellz-dev/rustyfin-server:latest` + `:sha-<short>`
  - `ghcr.io/jkellz-dev/rustyfin-meta:latest` + `:sha-<short>`

**release** (depends on: build, container-push, changelog; main only)
- git-cliff generates release notes since last tag
- `gh release create` with auto-tag, release notes, and binary artifacts attached

## Dockerfiles

Two minimal Dockerfiles — no multi-stage build since binaries are pre-built in CI.

**`docker/server.Dockerfile`**:
- Base: `debian:bookworm-slim`
- Installs: `ca-certificates`, `ffmpeg` (for transcoding)
- Copies pre-built `rustyfin-server` binary
- Entrypoint: `rustyfin-server`

**`docker/meta.Dockerfile`**:
- Base: `debian:bookworm-slim`
- Installs: `ca-certificates` (for HTTPS metadata fetching)
- Copies pre-built `rustyfin` binary
- Entrypoint: `rustyfin`

## git-cliff Configuration

**`cliff.toml`** at repo root.

Conventional commit grouping:
- `feat:` → Features
- `fix:` → Bug Fixes
- `refactor:` → Refactoring
- `docs:` → Documentation
- `test:` → Testing
- `chore:` → Miscellaneous

## Caching Strategy

- `Swatinem/rust-cache` on all Rust jobs (caches `target/` and registry)
- GitHub Actions cache for `trunk` binary
- Docker layer caching via `docker/build-push-action` cache options

## Files Created

- `.github/workflows/ci.yml` — single workflow file
- `docker/server.Dockerfile` — server container
- `docker/meta.Dockerfile` — meta CLI container
- `cliff.toml` — git-cliff configuration
