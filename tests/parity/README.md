# Parity suite

Black-box checks that Rustyfin answers like Jellyfin. The same Hurl files run
against a real Jellyfin and against Rustyfin; a case that passes on Jellyfin and
fails on Rustyfin is a parity gap.

Jellyfin is the oracle. `gen/record.py` asks it each question in `REQUESTS` and
writes what came back into `cases/*.hurl`: the status, the `Content-Type`, and the
JSON shape (each key present and of the same kind). Values that legitimately differ
(ids, times, names, counts) are not asserted. Do not edit the cases by hand; edit
`REQUESTS` and regenerate.

## Layout

- `bootstrap/jellyfin.hurl`, `bootstrap/rustyfin.hurl`: bring a fresh server to the
  same state (an admin user and three libraries over the synthetic media).
- `media/make_media.sh`: the synthetic media (a movie, a two-episode show, a
  two-track album), a few seconds each, fixed mtimes.
- `gen/record.py`: record Jellyfin and write the cases.
- `cases/*.hurl`: generated cases, one file per area.
- `run.py`: run every case against both servers and print the gaps.

## Running it

Both servers run in the `hermes-sandbox` namespace with a `hurl-runner` pod beside
them (the namespace fence stops anything outside from reaching them). Copy the media
into each server (`/media/parity-media` on Jellyfin, `/data/media/parity-media` on
Rustyfin), run the two bootstrap files from the runner (Rustyfin's with
`--variable media_root=/data/media/parity-media`), then:

```sh
python3 -I tests/parity/gen/record.py     # regenerate the cases from Jellyfin
python3 -I tests/parity/run.py            # report the gaps
python3 -I tests/parity/run.py library    # one area
```

`run.py` exits non-zero only if Jellyfin fails its own recording (the case is wrong,
not the server).

### Nightly, against Rustyfin alone

The cases are Jellyfin's recorded answers, so they also run with no Jellyfin at all. `.github/workflows/parity.yml` does
that every night (06:30 UTC) and on demand: the published `rustyfin-server:latest` image against a fresh Postgres, the
synthetic media from `media/make_media.sh`, `bootstrap/rustyfin.hurl` (with `media_root=/data/media/parity-media`),
`ci/wait_scanned.hurl` until the scan is in, then:

```sh
python3 -I tests/parity/run.py --local --server rustyfin --strict --all --write parity-report.md
```

`--local` runs `hurl` on this machine instead of the sandbox's runner pod
(`PARITY_RUSTYFIN_URL`, default `http://localhost:8096`); `--strict` fails on any
Rustyfin gap. The report is the job summary and an artifact. The sandbox run with
both servers is still how cases are recorded and Jellyfin's own pass is checked.

## Known differences

Deliberate departures from Jellyfin live in `KNOWN_DIFFERENCES` in `gen/record.py`,
each with the owner decision behind it. The recorder writes the matching assert as a
`# known difference, not asserted: ...` comment, so it stays visible in the case file.

- Tracks carry only `ProductionYear`, not `PremiereDate` (owner decision 2026-11-14).

## Known limits

- Shape only. A value that is wrong but of the right kind passes.
- Keys that Rustyfin returns and Jellyfin does not are not flagged.
- JSON responses only; playlists, images and streams are not covered yet.
- Items are recorded from one movie, one series and one album.
