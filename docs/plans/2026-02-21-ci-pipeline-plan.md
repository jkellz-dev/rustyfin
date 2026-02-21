# CI Pipeline Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** GitHub Actions CI/CD that gates PRs with lint+test, builds multi-arch containers on main push, and creates GitHub Releases with git-cliff notes.

**Architecture:** Single `.github/workflows/ci.yml` with parallel jobs (lint, test-unit, test-integration) gating a build→container-push→release pipeline. Two minimal Dockerfiles copy pre-built binaries into `debian:bookworm-slim`. git-cliff generates conventional-commit-based release notes.

**Tech Stack:** GitHub Actions, Docker buildx, `Swatinem/rust-cache`, `cross` (cross-compilation), `trunk` (WASM frontend), git-cliff, `gh release`

---

### Task 1: git-cliff Configuration

**Files:**
- Create: `cliff.toml`

**Step 1: Create `cliff.toml`**

```toml
[changelog]
header = """
# Changelog\n
"""
body = """
{% if version %}\
    ## [{{ version | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% else %}\
    ## [Unreleased]
{% endif %}\
{% for group, commits in commits | group_by(attribute="group") %}
    ### {{ group | striptags | trim | upper_first }}
    {% for commit in commits %}
        - {% if commit.scope %}*({{ commit.scope }})* {% endif %}\
            {% if commit.breaking %}[**breaking**] {% endif %}\
            {{ commit.message | upper_first }}\
    {% endfor %}
{% endfor %}\n
"""
footer = ""
trim = true

[git]
conventional_commits = true
filter_unconventional = true
split_commits = false
commit_parsers = [
    { message = "^feat", group = "Features" },
    { message = "^fix", group = "Bug Fixes" },
    { message = "^refactor", group = "Refactoring" },
    { message = "^doc", group = "Documentation" },
    { message = "^test", group = "Testing" },
    { message = "^perf", group = "Performance" },
    { message = "^style", group = "Styling" },
    { message = "^ci", group = "CI/CD" },
    { message = "^chore", group = "Miscellaneous" },
]
filter_commits = false
tag_pattern = "v[0-9].*"
sort_commits = "oldest"
```

**Step 2: Validate the config parses**

Run: `cargo install git-cliff` (if not already installed), then:
```bash
git-cliff --config cliff.toml --dry-run
```
Expected: outputs changelog content to stdout (may be empty if no conventional commits yet). No parse errors.

**Step 3: Commit**

```bash
jj describe -m "ci: add git-cliff configuration"
jj new
```

---

### Task 2: Server Dockerfile

**Files:**
- Create: `docker/server.Dockerfile`

**Step 1: Create `docker/server.Dockerfile`**

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY rustyfin-server /usr/local/bin/rustyfin-server

EXPOSE 8096

ENTRYPOINT ["rustyfin-server"]
```

**Step 2: Validate Dockerfile syntax**

Run:
```bash
docker build --check -f docker/server.Dockerfile docker/ 2>&1 || echo "Syntax check not supported, visual inspection only"
```

No syntax errors. (Note: `--check` requires Docker 27+. If not available, visual inspection is sufficient — CI will catch build errors.)

**Step 3: Commit**

```bash
jj describe -m "ci: add server Dockerfile"
jj new
```

---

### Task 3: Meta CLI Dockerfile

**Files:**
- Create: `docker/meta.Dockerfile`

**Step 1: Create `docker/meta.Dockerfile`**

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY rustyfin /usr/local/bin/rustyfin

ENTRYPOINT ["rustyfin"]
```

**Step 2: Commit**

```bash
jj describe -m "ci: add meta CLI Dockerfile"
jj new
```

---

### Task 4: CI Workflow — Lint Job

**Files:**
- Create: `.github/workflows/ci.yml`

This task creates the workflow file with the first job. Subsequent tasks add more jobs to the same file.

**Step 1: Create `.github/workflows/ci.yml` with lint job**

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2

      - name: Check formatting
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy -- -D warnings
```

**Step 2: Validate YAML syntax**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```
Expected: `YAML OK`

**Step 3: Commit**

```bash
jj describe -m "ci: add workflow with lint job"
jj new
```

---

### Task 5: CI Workflow — Test Jobs

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add test-unit and test-integration jobs**

Append after the `lint` job (inside `jobs:`):

```yaml
  test-unit:
    name: Unit Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Run unit tests
        run: cargo test --lib

  test-integration:
    name: Integration Tests
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:17
        env:
          POSTGRES_DB: rustyfin_test
          POSTGRES_USER: rustyfin
          POSTGRES_PASSWORD: rustyfin
        ports:
          - 5432:5432
        options: >-
          --health-cmd="pg_isready -U rustyfin"
          --health-interval=10s
          --health-timeout=5s
          --health-retries=5
    env:
      DATABASE_URL: postgres://rustyfin:rustyfin@localhost:5432/rustyfin_test
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Run integration tests
        run: cargo test -p rustyfin-api --test '*' -- --test-threads=1
```

**Step 2: Validate YAML syntax**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```
Expected: `YAML OK`

**Step 3: Commit**

```bash
jj describe -m "ci: add unit and integration test jobs"
jj new
```

---

### Task 6: CI Workflow — Frontend Build Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add build-frontend job**

Append after `test-integration` job:

```yaml
  build-frontend:
    name: Build Frontend
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Install trunk
        uses: taiki-e/install-action@v2
        with:
          tool: trunk

      - name: Build frontend
        working-directory: frontend
        run: trunk build --release

      - uses: actions/upload-artifact@v4
        with:
          name: frontend-dist
          path: frontend/dist/
          retention-days: 1
```

**Step 2: Validate YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

**Step 3: Commit**

```bash
jj describe -m "ci: add frontend build job"
jj new
```

---

### Task 7: CI Workflow — Binary Build Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add build job with cross-compilation**

Append after `build-frontend` job:

```yaml
  build:
    name: Build (${{ matrix.target }})
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    needs: [lint, test-unit, test-integration, build-frontend]
    strategy:
      matrix:
        target:
          - x86_64-unknown-linux-gnu
          - aarch64-unknown-linux-gnu
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - name: Install cross
        uses: taiki-e/install-action@v2
        with:
          tool: cross

      - uses: actions/download-artifact@v4
        with:
          name: frontend-dist
          path: frontend/dist/

      - name: Build binaries
        run: cross build --release --target ${{ matrix.target }}

      - name: Prepare artifacts
        run: |
          mkdir -p artifacts
          cp target/${{ matrix.target }}/release/rustyfin-server artifacts/rustyfin-server-${{ matrix.target }}
          cp target/${{ matrix.target }}/release/rustyfin artifacts/rustyfin-${{ matrix.target }}

      - uses: actions/upload-artifact@v4
        with:
          name: binaries-${{ matrix.target }}
          path: artifacts/
          retention-days: 1
```

**Step 2: Validate YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

**Step 3: Commit**

```bash
jj describe -m "ci: add cross-compiled binary build job"
jj new
```

---

### Task 8: CI Workflow — Container Push Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add permissions block at workflow level**

Add after the `env:` block at the top of the file:

```yaml
permissions:
  contents: write
  packages: write
```

**Step 2: Add container-push job**

Append after `build` job:

```yaml
  container-push:
    name: Push Containers
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    needs: [build]
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          name: binaries-x86_64-unknown-linux-gnu
          path: artifacts/amd64/

      - uses: actions/download-artifact@v4
        with:
          name: binaries-aarch64-unknown-linux-gnu
          path: artifacts/arm64/

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Prepare server context
        run: |
          mkdir -p docker/context/server/linux/{amd64,arm64}
          cp artifacts/amd64/rustyfin-server-x86_64-unknown-linux-gnu docker/context/server/linux/amd64/rustyfin-server
          cp artifacts/arm64/rustyfin-server-aarch64-unknown-linux-gnu docker/context/server/linux/arm64/rustyfin-server
          chmod +x docker/context/server/linux/*/rustyfin-server

      - name: Prepare meta context
        run: |
          mkdir -p docker/context/meta/linux/{amd64,arm64}
          cp artifacts/amd64/rustyfin-x86_64-unknown-linux-gnu docker/context/meta/linux/amd64/rustyfin
          cp artifacts/arm64/rustyfin-aarch64-unknown-linux-gnu docker/context/meta/linux/arm64/rustyfin
          chmod +x docker/context/meta/linux/*/rustyfin

      - name: Build & push server image
        uses: docker/build-push-action@v6
        with:
          context: docker/context/server
          file: docker/server.Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: |
            ghcr.io/jkellz-dev/rustyfin-server:latest
            ghcr.io/jkellz-dev/rustyfin-server:sha-${{ github.sha }}

      - name: Build & push meta image
        uses: docker/build-push-action@v6
        with:
          context: docker/context/meta
          file: docker/meta.Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: |
            ghcr.io/jkellz-dev/rustyfin-meta:latest
            ghcr.io/jkellz-dev/rustyfin-meta:sha-${{ github.sha }}
```

**Important note on multi-arch Dockerfiles:** The Dockerfiles from Tasks 2 and 3 need a small tweak to `COPY` from the arch-specific subdirectory. Update them to use `TARGETARCH` build arg:

Update `docker/server.Dockerfile`:
```dockerfile
FROM debian:bookworm-slim

ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY linux/${TARGETARCH}/rustyfin-server /usr/local/bin/rustyfin-server

EXPOSE 8096

ENTRYPOINT ["rustyfin-server"]
```

Update `docker/meta.Dockerfile`:
```dockerfile
FROM debian:bookworm-slim

ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY linux/${TARGETARCH}/rustyfin /usr/local/bin/rustyfin

ENTRYPOINT ["rustyfin"]
```

**Step 3: Validate YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

**Step 4: Commit**

```bash
jj describe -m "ci: add container push job with multi-arch support"
jj new
```

---

### Task 9: CI Workflow — Release Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add release job**

Append after `container-push` job:

```yaml
  release:
    name: GitHub Release
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    needs: [container-push]
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install git-cliff
        uses: taiki-e/install-action@v2
        with:
          tool: git-cliff

      - name: Generate release notes
        id: changelog
        run: |
          # Generate notes since last tag (or all history if no tags)
          NOTES=$(git-cliff --config cliff.toml --latest --strip header 2>/dev/null || git-cliff --config cliff.toml --strip header)
          echo "notes<<EOF" >> "$GITHUB_OUTPUT"
          echo "$NOTES" >> "$GITHUB_OUTPUT"
          echo "EOF" >> "$GITHUB_OUTPUT"

      - name: Generate version tag
        id: version
        run: |
          VERSION="v0.1.0-$(date +%Y%m%d)-$(echo ${{ github.sha }} | cut -c1-7)"
          echo "tag=$VERSION" >> "$GITHUB_OUTPUT"

      - uses: actions/download-artifact@v4
        with:
          pattern: binaries-*
          merge-multiple: true
          path: release-artifacts/

      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "${{ steps.version.outputs.tag }}" \
            --title "${{ steps.version.outputs.tag }}" \
            --notes "${{ steps.changelog.outputs.notes }}" \
            --prerelease \
            release-artifacts/*
```

**Step 2: Validate YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

**Step 3: Commit**

```bash
jj describe -m "ci: add GitHub Release job with git-cliff notes"
jj new
```

---

### Task 10: Cross Configuration

**Files:**
- Create: `Cross.toml`

The `cross` tool needs a configuration file to ensure it uses a compatible Docker image for aarch64 cross-compilation.

**Step 1: Create `Cross.toml`**

```toml
[target.aarch64-unknown-linux-gnu]
image = "ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main"
```

**Step 2: Commit**

```bash
jj describe -m "ci: add Cross.toml for aarch64 cross-compilation"
jj new
```

---

### Task 11: Final Validation & Squash

**Step 1: Review the complete workflow file**

Read through `.github/workflows/ci.yml` end-to-end and verify:
- All job names are unique
- `needs:` references match actual job names
- `if:` conditions on main-only jobs are consistent
- Artifact names in upload/download match
- GHCR image names are correct (`ghcr.io/jkellz-dev/...`)

**Step 2: Validate full YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

**Step 3: Run lint locally to confirm the workflow wouldn't fail immediately**

Run:
```bash
cargo fmt --check && cargo clippy -- -D warnings
```
Expected: both pass

**Step 4: Final commit**

Squash all commits into a single clean commit:

```bash
jj describe -m "ci: GitHub Actions pipeline with lint, test, build, containers, and releases"
```

---

## Reference: Complete File List

| File | Purpose |
|------|---------|
| `.github/workflows/ci.yml` | Single CI/CD workflow |
| `docker/server.Dockerfile` | Server container (debian-slim + ffmpeg) |
| `docker/meta.Dockerfile` | Meta CLI container (debian-slim) |
| `cliff.toml` | git-cliff changelog config |
| `Cross.toml` | cross-rs configuration for aarch64 |

## Reference: Environment Secrets

No additional secrets needed — `GITHUB_TOKEN` is automatically available and provides:
- GHCR push access (via `packages: write` permission)
- Release creation (via `contents: write` permission)

## Reference: First Push Checklist

After merging to main, verify:
1. Lint job passes (fmt + clippy)
2. Unit test job passes
3. Integration test job passes (Postgres service container works)
4. Frontend builds successfully with trunk
5. Cross-compilation produces binaries for both architectures
6. Container images appear in GitHub Packages (private)
7. GitHub Release is created with changelog and binary artifacts
