# Rustyfin: rules for code changes

Rustyfin is a media server in Rust. These rules are binding for every change. The full picture is in
[docs/architecture.md](docs/architecture.md).

## Layers

The server is built in layers. Each layer depends only on the ones below it.

```text
rustyfin-server            glue: config, startup, builds the data layer, mounts the API routers
├── rustyfin-jellyfin      the Jellyfin API and everything Jellyfin compatibility needs
├── rustyfin-api           the Rustyfin API
└── rustyfin-services      all runtime business logic and rules
    └── rustyfin-data      the only crate that talks to a database
        rustyfin-models    models and data-layer traits shared by services and data
```

### `rustyfin-data`

- It is the only crate that touches the database. Database types never leave it: sqlx, pools, transactions, row
  structs, SQL errors.
- It implements the data traits that `rustyfin-services` consumes. Another data layer must be possible by writing
  another implementation of those traits.
- It has its own models, used for data only.
- SQL stays SQL. Filtering, joining and sorting belong in queries; do not hoist database logic out of them.
- Meilisearch (the search index) also lives here.

### `rustyfin-models`

- Models shared by services and data, and the data traits.
- When a service needs a model shaped differently from the data model, the service gets its own model. Never bend a
  shared model to suit one side.

### `rustyfin-services`

- All runtime business logic and rules for the Rustyfin ecosystem: scans, linked servers, sessions, permissions,
  playback decisions, and anything else that decides something.
- No HTTP and no serialisation formats. Services return typed results and their own error type.
- Services are generic over the data traits. They never name a concrete database.

### `rustyfin-jellyfin`

- The Jellyfin API and everything needed to make Jellyfin clients work: routes, Jellyfin's JSON shapes, its error
  shape, ID format, case rules, stubs and no-op sign-in. It has its own models.
- Serialisation and translation only. It calls services for anything that decides something.
- Removing Jellyfin support must be removing this crate and its line in `rustyfin-server`. Nothing outside it may
  depend on it or copy its conventions.

### `rustyfin-api`

- The Rustyfin API, under `/api/v1`. Serialisation only; it calls services for all runtime logic.
- Its models are completely separate from the data-layer models. Its wire models live in a serde-only crate the client
  can share.
- It makes its own design decisions (error shape, naming, formats). Nothing in it is inherited from Jellyfin.

### Between the two APIs

- `rustyfin-jellyfin` and `rustyfin-api` share no HTTP code: each owns its request identity, error responses and routing
  rules. They share only services and models.
- Metrics and tracing are applied to both routers by `rustyfin-server`.

## Where new code goes

- New SQL goes in `rustyfin-data`, behind a trait method.
- New logic goes in `rustyfin-services`, never in a route handler.
- New Jellyfin behaviour goes in `rustyfin-jellyfin`; new Rustyfin API routes go in `rustyfin-api`.
- An API crate never depends on `rustyfin-data`.

## Migration status

The code is moving to this layout one area at a time. Done: `rustyfin-db` is now `rustyfin-data`; database rows are
private to it, behind one trait per area in `rustyfin_models::data`. `rustyfin-core` is now `rustyfin-services`, generic
over `Data`, and holds the rules (access, users, libraries, playback, sessions, shows, browsing, tokens, API keys,
collections, scans, links). `rustyfin-api`, `rustyfin-auth`, `rustyfin-search` and `rustyfin-media` no longer depend
on `rustyfin-data`; the API is generic over `D: Data` and only `rustyfin-server` picks Postgres. Still to move: the
split of `rustyfin-api` into `rustyfin-jellyfin` and the new `rustyfin-api`, and Meilisearch into the data layer. Code
you touch moves towards the layout; never add new direct database access outside `rustyfin-data`.
