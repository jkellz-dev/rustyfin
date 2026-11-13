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
Rustyfin), run the two bootstrap files from the runner, then:

```sh
python3 -I tests/parity/gen/record.py     # regenerate the cases from Jellyfin
python3 -I tests/parity/run.py            # report the gaps
python3 -I tests/parity/run.py library    # one area
```

`run.py` exits non-zero only if Jellyfin fails its own recording (the case is wrong,
not the server).

## Known limits

- Shape only. A value that is wrong but of the right kind passes.
- Keys that Rustyfin returns and Jellyfin does not are not flagged.
- JSON responses only; playlists, images and streams are not covered yet.
- Items are recorded from one movie, one series and one album.
