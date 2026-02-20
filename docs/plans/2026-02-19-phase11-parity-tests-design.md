# Phase 11a + 11b — Jellyfin Parity Audit & Integration Test Suite

## Overview

Two complementary work areas:
- **11a**: Systematic controller-by-controller comparison of rustyfin responses against Jellyfin source. Fix shape mismatches, add missing stubs, implement individual item endpoints.
- **11b**: Integration test infrastructure with real Postgres, seeded test data, and in-process router testing via `tower::ServiceExt::oneshot`.

## 11b — Integration Test Infrastructure

### Architecture

A `TestApp` harness in `rustyfin-api` that constructs a real `AppState` with a live Postgres pool. Each test creates an isolated schema, runs migrations, seeds data, exercises the router, and drops the schema on cleanup.

### Test Isolation: Per-Test Random Schema

Each test creates a Postgres schema with a random name (e.g., `test_abc123`), runs migrations within it, and drops it on cleanup. This allows parallel test execution without data conflicts. The pool uses `SET search_path` to scope all queries to the test schema.

### TestApp Struct

```rust
struct TestApp {
    router: Router,
    state: AppState,
    pool: PgPool,
    schema_name: String,
}

impl TestApp {
    async fn new() -> Self { /* create schema, migrate, build AppState + Router */ }
    async fn create_user(&self, name: &str, is_admin: bool) -> (Uuid, String); // (user_id, token)
    async fn create_library(&self, name: &str, path: &str) -> Uuid;
    async fn create_media_item(&self, ...) -> Uuid;
    fn get(&self, path: &str, token: &str) -> Request<Body>;
    fn post(&self, path: &str, body: Value, token: &str) -> Request<Body>;
    async fn send(&self, req: Request<Body>) -> Response<Body>;
}

impl Drop for TestApp { /* DROP SCHEMA test_abc123 CASCADE */ }
```

### Dev-Dependencies

Added to workspace `Cargo.toml`:
- `tower` — `ServiceExt::oneshot` for in-process request dispatch
- `hyper` — `Request`/`Body` building
- `serde_json` — response parsing and assertion

### Auth in Tests

Create real sessions in the DB with real tokens. The `AuthenticatedUser` extractor queries `sessions` + `users` tables, so seeded data works without mocking. No forward auth bypass needed — tests inject `X-Emby-Token` headers directly.

### Test Groups

**Group 1: Auth enforcement**
- Unauthenticated request → 401 on protected endpoints
- Non-admin on admin endpoints (`POST /Library/Refresh`, `POST /System/Configuration`, `GET /Plugins`) → 403
- Admin on admin endpoints → 200
- `GET /Sessions` as admin → all sessions; non-admin → only own
- Disabled user token → 401
- Legacy routes delegate correctly

**Group 2: Core CRUD**
- `GET /Users` returns seeded users with PascalCase
- `GET /Users/Me` returns authenticated user
- `GET /Items` with filters (genre, year, parentId, recursive)
- `GET /Items/{id}` with media streams and image tags
- Playback reporting: Playing → Progress → Stopped → user_data updated
- User data: favorites, played status

**Group 3: DTO shape validation**
- Required fields present and correctly cased
- `QueryResult` wrapping (`Items`, `TotalRecordCount`, `StartIndex`)
- Empty results: `[]` not null

**Group 4: Edge cases**
- Malformed UUID → 400
- Non-existent item → 404
- Empty body where body expected → 400

## 11a — Parity Audit

### Method

Controller-by-controller comparison of rustyfin handlers against Jellyfin source (`/home/jonathan/src/jellyfin/Jellyfin.Api/Controllers/`). Both codebases indexed (rustyfin via coderlm, Jellyfin via claude-context).

### Focus Areas (client-impacting gaps)

| Area | Current | Gap |
|------|---------|-----|
| Images | 3/30 | User avatars, genre/studio/person images |
| Sessions | 9/16 | Now-playing broadcast, viewing |
| Users | 7/18 | Password change stubs, user image |
| Subtitles | 3/10 | Search/upload stubs |
| Genres/Studios/Persons | 3/6 | Individual GET by name/ID |

### Deliverables

1. **Missing stub endpoints** — add stubs where clients hit 404s (empty/default responses, same pattern as Phase 6b)
2. **Response shape fixes** — missing fields, wrong nullability, wrong defaults found during comparison
3. **Individual item endpoints** — `GET /Genres/{name}`, `GET /Studios/{name}`, `GET /Persons/{name}` with real DB queries

### Not In Scope

Full implementation of excluded features (LiveTV, SyncPlay, Plugins). Only stubs to prevent client breakage.

## Decisions

- Per-test random Postgres schemas for isolation (not transactions, not testcontainers)
- Reuse existing dev docker-compose Postgres (no separate test container)
- Integration tests before unit tests (higher value for parity confidence)
- Parity audit scoped to client-impacting gaps, not full 398-endpoint coverage
