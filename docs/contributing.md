# Contributing

How changes are made in this repository. Setting up a machine is in the [Development Guide](development.md); what is
planned, and the design boundaries every change must respect, are at the top of [`roadmap.md`](../roadmap.md).

## Before you start

- **Check the roadmap.** Pick an open item, or add one for a new idea before building it. Changes that cross a design
  boundary (forward auth only; no plugins, LiveTV, photos or books; the server never writes to media folders) need the
  maintainer's agreement first.
- **Check Jellyfin.** Rustyfin answers Jellyfin clients, so a route's shape and rules follow Jellyfin's (its
  integration tests and source are the spec). Where Rustyfin adds something of its own, say so in the docs.

## Making a change

1. **Write the test first and see it fail.** Server routes are tested in `crates/rustyfin-api/tests/` against a real
   Postgres; logic elsewhere gets unit tests next to it; the admin's logic is tested on the host with `cargo test` in
   `frontend/`. Commit the failing test on its own (`test: ...`), so the red run is on record.
2. **Make it pass, with no warnings.** All of these must pass:

   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   DATABASE_URL=postgres://rustyfin:rustyfin@localhost:5432/rustyfin_test cargo test --workspace
   cd frontend && cargo test && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings
   ```

   Lint findings are fixed, not silenced with `#[allow]`.
3. **Update the docs and the roadmap in the same change**: tick the roadmap items it finishes, and change the page in
   `docs/` that describes what you changed.
4. **Migrations only add.** A migration that drops or rewrites existing data needs the maintainer's agreement first:
   people's watch history lives in that database.

## Commits and history

- **Conventional commits**: `feat:`, `fix:`, `test:`, `docs:`, `refactor:`, `perf:`, `chore:`, with a scope where it
  helps (`feat(admin): ...`). The body says what changed and why. `CHANGELOG.md` is generated from these by git-cliff
  (`cliff.toml`), so write the subject for a reader of the changelog.
- **Linear history.** `main` has no merge commits. Rebase your branch onto `main` (never merge `main` into it); it
  lands with a fast-forward.
- **Pre-commit checks** run through [hk](https://hk.jdx.dev) (`hk.pkl`): `cargo fmt`, Markdown (rumdl), the admin's
  HTML and CSS (oxfmt), shell scripts (shellcheck, shellharden, shfmt), YAML and GitHub workflows (yamlfmt, zizmor).
  Install the hook with `hk install`; `hk fix --all` fixes what it can.

## What CI runs

On every pull request and push (`.github/workflows/ci.yml`): formatting and clippy, unit tests, the integration tests
against Postgres 17, and the admin build with trunk. Pushes to `main` also build the release binaries and publish the
`rustyfin-server` and `rustyfin-meta` images to `ghcr.io/jkellz-dev/` (tags `sha-<commit>` and `latest`); a `v*` tag
adds a version tag and a GitHub release with the changelog. `parity.yml` runs nightly: the Hurl parity suite against
the published image, checked against Jellyfin's recorded answers (`tests/parity`).

## Writing

Plain, specific sentences: what it does, what the user sees, why. Name the file, route or setting. In docs, say what
Rustyfin does today rather than what it might do.
