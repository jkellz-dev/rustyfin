# Linked Servers

A friend's Jellyfin (or Rustyfin) can share some of its libraries with your Rustyfin. Their films then appear in your
libraries next to yours, as one collection:

- A film you already have gains their copy as another version. The best copy is listed first and plays unless you pick
  another (see [Which copy plays](#which-copy-plays)).
- A film you do not have is listed like your own, with its artwork, details and provider ids.
- Every version the friend keeps of a film is a copy of its own ("2160p Director's Cut · Alex").
- A film of yours that lacks artwork, an overview, genres, ratings or provider ids borrows them from a friend's copy.
  What your film has is never replaced.
- Your watched state, resume points and favourites stay on your server, whichever copy you played.

Your apps only ever talk to your server. It signs in to the friend's server, keeps a copy of the shared catalogue (never
the media), and passes the friend's posters and video bytes through. Nothing changes for the apps: a linked film is an
ordinary film with more than one version.

## What works so far

| Works | Not yet |
|-------|---------|
| Films from a current Jellyfin (tested with 10.10) | TV shows, music |
| Direct play of a friend's copy, with seeking | Transcoding a friend's copy |
| Posters, backdrops, logos and thumbnails | A friend's external subtitle files, chapters, cast |
| Every version the friend keeps of a film | Link status in the admin (it is in the server log for now) |
| Several links, one per friend | A faster Rustyfin-to-Rustyfin sync (a Rustyfin is linked through its Jellyfin API) |

Since a friend's copy cannot be transcoded yet, it is offered only to apps that can play it as is. An app that cannot
gets your own copy first; a film only the friend has will not play in that app until transcoding through links lands.
The [roadmap](../roadmap.md) (16b) tracks the rest.

## What your friend does

Your friend makes a user on their server for your link, and lets it see only the libraries they want to share.

In Jellyfin:

1. **Dashboard → Users → +**: a name such as `link-for-sam` and a strong password.
2. On the new user's **Access** tab, untick **Enable access to all libraries** and tick the libraries to share.
3. On its **Profile** tab, leave **Allow this user to manage the server** unticked. Turning off remote control,
   media deletion and downloads does no harm: the link only reads the catalogue and streams.
4. Send you the server's address, the user name and the password, by a channel you would trust with a password.

An administrator's API key is not needed and should not be used: it would give your server everything on theirs.

Your friend can stop sharing at any time by disabling or deleting the user, or by unticking a library. Your server
drops what it can no longer see at its next sync.

## What you do

Add a `[[link]]` for each friend to `rustyfin.toml`, then restart Rustyfin:

```toml
[[link]]
name = "Alex"                          # shown beside their copies: "2160p · Alex"
kind = "jellyfin"                      # what the friend runs: "jellyfin" or "rustyfin"
url = "https://jellyfin.alex.example"  # as your server reaches it
user = "link-for-sam"
password_file = "/run/secrets/alex-link-password"   # or: password = "..."

[[link.libraries]]
remote = "Films"                       # the friend's library, by name (case does not matter)
local = "Movies"                       # one of your libraries, by name

[[link.libraries]]                     # as many as you like
remote = "4K Films"
local = "Movies"
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | The link's name, unique; shown to your users beside the friend's copies |
| `kind` | yes | `jellyfin` or `rustyfin` |
| `url` | yes | The friend's server as your server reaches it, with `https://` or `http://` |
| `user` | yes | The user your friend made for the link |
| `password` / `password_file` | one of them | The user's password, or a file holding it (a trailing newline is ignored) |
| `libraries` | at least one | `remote` (the friend's library) merges into `local` (yours) |

Prefer `password_file` with a secret mounted into the container (a Docker or Kubernetes secret) over writing the
password into the config file. Environment variables cannot set `[[link]]` tables.

A link with a missing password or no libraries is skipped at start with an error in the log; the server still starts.
A `local` library must already exist on your server; map the friend's films to a films library.

### Reaching the friend's server

Your server, not your apps, connects to the friend's. It needs to reach their `url` from where it runs: over the
internet with HTTPS, or across a private network such as a VPN or a Tailscale node your friend shares with you. Plain
`http://` is only for servers on a network you trust: the link's password and the media would cross it unencrypted.

Playback of a friend's copy runs through your server, so it uses your server's bandwidth twice (in from the friend,
out to your app) and the friend's upload speed limits it.

## How syncing works

Your server syncs each link 30 seconds after it starts and then every 15 minutes:

1. It signs in as the link user (and signs in again if the friend's server forgot the session).
2. For each mapped library, it lists the friend's films with Jellyfin's change tag (`Etag`). This is a light request.
3. It fetches the full details of new and changed films only, 100 at a time.
4. It removes the films, and the versions of films, the friend no longer shares, and the artwork your films borrowed
   from them.
5. It merges (below).

A sync with nothing changed costs one light request per library. A library scan on your server merges too, so a film
you add takes in the friend's copy as soon as the scan finds it.

If a sync fails (wrong password, the friend's server is down, a library name that does not match), the server log says
why and the films from the last good sync stay. The log lines start with `Link <name>`:

```text
INFO  rustyfin_core::link: Link Alex: 12 added, 3 updated, 1 removed, 9 merged in 1.2s
WARN  rustyfin_core::link: Link Alex did not sync: could not sign in to https://jellyfin.alex.example as link-for-sam: 401 Unauthorized
WARN  rustyfin_core::link: Link Alex did not sync: Alex shares no library named "Films" with its link user (it shares: Movies)
```

The `links` table holds each link's last good sync and last error (see [Database Schema](database-schema.md)).

## How copies are matched

Only by provider id: the same TMDB or IMDb id. Titles and years are never compared, because a wrong match would hide a
film behind another. So:

- A film your server knows only by its title (no TMDB or IMDb id from its folder name, NFO or
  [`.rustyfin.toml`](title-settings.md)) is never merged; it is listed beside the friend's copy. Give it an id and the
  next scan merges it.
- A friend's film whose ids match two of your films stays on its own.
- Two friends' copies of a film you lack are one film, and so are the versions one friend keeps of it.

## Which copy plays

All copies of a film, yours and your friends', are ranked by:

1. resolution (a wide film is judged by its width, so 1920×800 counts as 1080p);
2. dynamic range: Dolby Vision, then HDR10 and HDR10+, then HLG, then SDR;
3. video bitrate, weighed by how efficient the codec is (AV1 counts double, HEVC one and a half times, VP9 1.4 times,
   against H.264);
4. audio channels;
5. lossless audio over lossy.

The best copy is listed first and plays when an app does not pick one; on a tie, your own copy wins. An app's version
menu lists every copy, the friend's named with the link's name. A copy the app cannot play as is (a friend's copy, for
now) is listed after every copy it can play.

## What your friend sees

One signed-in device named "Rustyfin link" for the link user, and one stream per film playing through the link. Your
users' names and what they watch stay on your server.

## Removing a link

Delete its `[[link]]` and restart: it stops syncing. Its films stay listed, but no longer play, until the link is
removed from the database too:

```sql
DELETE FROM images WHERE path LIKE 'link://' || (SELECT id FROM links WHERE name = 'Alex') || '/%';
DELETE FROM links WHERE name = 'Alex';
```

The first line removes the artwork your films borrowed from the friend's copies; the second removes the link and every
copy it brought (and the watched state of films only the friend had). Your own films are never touched. Removing a
link from the config should do this by itself; that is on the [roadmap](../roadmap.md).
