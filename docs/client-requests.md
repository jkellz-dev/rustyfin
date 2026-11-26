# What clients ask for

Roadmap 11a: rank the requests real clients make, and find the ones Rustyfin
does not answer. The request log and a ranking script do it.

## Collecting

Turn on the request log (one line per request: method, path and query with
credentials replaced by `***`, status, time) and use the client for a while,
covering every screen:

```sh
# on the server: RUST_LOG=rustyfin=info,tower_http=info,rustyfin_server::request_log=debug
docker compose logs --no-log-prefix rustyfin > live.log
python3 -I tests/parity/tools/rank_requests.py live.log tests/parity/cases
```

The script groups requests by route shape (ids replaced), most frequent first,
and for each one prints its p95 time, its statuses, whether a parity case
covers it, and the query parameters no case sends. Routes Rustyfin does not
have are also logged at `info` as `No route for <method> <path>`.

Fix what it finds with a parity case in `tests/parity/cases` (see `tests/parity/README.md`), so the request stays
answered.
