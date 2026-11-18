# Linked Servers

A friend's Jellyfin (or Rustyfin) can share some of its libraries with your Rustyfin. Their films and shows then
appear in your libraries next to yours, as one collection:

- A film you already have gains their copy as another version. The best copy is listed first and plays unless you pick
  another (see [Which copy plays](#which-copy-plays)).
- A film you do not have is listed like your own, with its artwork, details and provider ids.
- A show you already have gains the seasons and episodes you lack, and their copy of each episode you both have. A
  show you do not have is listed whole.
- A library of theirs with no counterpart on your server (home videos, a collection of shorts) can get a library of
  its own.
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
| Films, TV shows and home videos from a current Jellyfin (tested with 10.10) | |
| Direct play of a friend's copy, with seeking; transcoding by the friend's server | Music |
| Posters, backdrops, logos and thumbnails | A friend's external subtitle files, chapters, cast |
| Every version the friend keeps of a film | |
| Several links, one per friend; their state in the admin's Links page | A faster Rustyfin-to-Rustyfin sync (a Rustyfin is linked through its Jellyfin API) |

An app that cannot play a friend's copy as is gets it converted by the friend's server, which has the file; your
server passes the stream on. Your server never converts a friend's copy itself. The [roadmap](../roadmap.md) (16b)
tracks the rest.

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

[[link.libraries]]
remote = "Shows"
local = "TV Shows"

[[link.libraries]]                     # no local: a library of its own,
remote = "Homemade Shorts"             # "Homemade Shorts · Alex"
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | The link's name, unique; shown to your users beside the friend's copies |
| `kind` | yes | `jellyfin` or `rustyfin` |
| `url` | yes | The friend's server as your server reaches it, with `https://` or `http://` |
| `user` | yes | The user your friend made for the link |
| `password` / `password_file` | one of them | The user's password, or a file holding it (a trailing newline is ignored) |
| `libraries` | at least one | `remote` (the friend's library) merges into `local` (yours); with no `local`, it gets a library of its own |

Prefer `password_file` with a secret mounted into the container (a Docker or Kubernetes secret) over writing the
password into the config file. Environment variables cannot set `[[link]]` tables.

A link with a missing password or no libraries is skipped at start with an error in the log; the server still starts.
A `local` library must already exist on your server (a misspelt name is an error, never a new library). Map films to
a films library and shows to a shows library.

### A library of its own

A mapping with no `local` gets a library of its own on your server, named after the friend's library and the link
("Homemade Shorts · Alex"), of the same kind (shows for a shows library, films for anything else). It has no folders:
scans leave it alone, and only the link fills it. It shows in your apps like your other libraries, and
[per-user library access](configuration.md) applies to it. Home videos (Jellyfin's "Home Videos" libraries) come across
as films; they have no provider ids, so they are never merged with anything.

### Reaching the friend's server

Your server, not your apps, connects to the friend's. It needs to reach their `url` from where it runs: over the
internet with HTTPS, or across a private network such as a VPN or a Tailscale node your friend shares with you. Plain
`http://` is only for servers on a network you trust: the link's password and the media would cross it unencrypted.

Playback of a friend's copy runs through your server, so it uses your server's bandwidth twice (in from the friend,
out to your app) and the friend's upload speed limits it. A converted copy uses the friend's CPU (or GPU), counts
against their server's transcode limits, and stops when their server sees it idle.

## How syncing works

Your server syncs each link 30 seconds after it starts and then every 15 minutes:

1. It signs in as the link user (and signs in again if the friend's server forgot the session).
2. For each mapped library, it lists the friend's films (or series, seasons and episodes) with Jellyfin's change tag
   (`Etag`). This is a light request.
3. It fetches the full details of new and changed items only, 100 at a time.
4. It removes what the friend no longer shares, and the artwork your titles borrowed from it.
5. It merges (below).

A mapping that cannot sync (a library the friend no longer shares with the link user, a local library that does not
exist) is skipped with a warning; the link's other libraries still sync. While any mapping fails, nothing is removed:
its titles may only be out of sight for a moment.

A sync with nothing changed costs one light request per library. A library scan on your server merges too, so a film
you add takes in the friend's copy as soon as the scan finds it.

If a sync fails (wrong password, the friend's server is down, a library name that does not match), the server log says
why and the films from the last good sync stay. The log lines start with `Link <name>`:

```text
INFO  rustyfin_core::link: Link Alex: 12 added, 3 updated, 1 removed, 9 merged in 1.2s
WARN  rustyfin_core::link: Link Alex did not sync: could not sign in to https://jellyfin.alex.example as link-for-sam: 401 Unauthorized
WARN  rustyfin_core::link: Link Alex did not sync: Alex shares no library named "Films" with its link user (it shares: Movies)
```

The admin's **Links** page (`/web/links`) shows each link: whether its last sync worked and why not, when it last
synced, how many titles only it brought and how many copies it holds, with a **Sync now** button. The same is
`GET /Rustyfin/Links` for administrators; `POST /Rustyfin/Links/{id}/Sync` starts a sync.

## How copies are matched

Only by provider id: a film by its TMDB or IMDb id, a show by its TVDB, TMDB or IMDb id. Titles and years are never
compared, because a wrong match would hide a title behind another. Within a show, seasons match by season number and
episodes by episode number. So:

- A film your server knows only by its title (no TMDB or IMDb id from its folder name, NFO or
  [`.rustyfin.toml`](title-settings.md)) is never merged; it is listed beside the friend's copy. Give it an id and the
  next scan merges it.
- A friend's film whose ids match two of your films stays on its own.
- Two friends' copies of a film you lack are one film, and so are the versions one friend keeps of it. The same goes
  for shows.
- A friend's season you lack goes into your show; a friend's episode you lack goes into your season; a friend's copy
  of an episode you have is another version of yours.

## Which copy plays

All copies of a film or episode, yours and your friends', are ranked by:

1. resolution (a wide film is judged by its width, so 1920×800 counts as 1080p);
2. dynamic range: Dolby Vision, then HDR10 and HDR10+, then HLG, then SDR;
3. video bitrate, weighed by how efficient the codec is (AV1 counts double, HEVC one and a half times, VP9 1.4 times,
   against H.264);
4. audio channels;
5. lossless audio over lossy.

The best copy is listed first and plays when an app does not pick one; on a tie, your own copy wins. An app's version
menu lists every copy, the friend's named with the link's name. A copy the app can play neither as is nor converted
(the friend's server is down) is listed after every copy it can play.

## What your friend sees

One signed-in device named "Rustyfin link" for the link user, and one stream (or transcode) per title playing through
the link. Your users' names and what they watch stay on your server.

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
