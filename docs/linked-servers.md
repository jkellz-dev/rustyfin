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
- A friend's film joins your collection of its series (Alien³ joins your "Alien Collection"), and a collection only
  your friend has comes across ([Collections](#collections)).
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
| Several links, one per friend; their state in the admin's Links page | |
| A Rustyfin through forward auth, synced from its change feed | |

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
| `user` | Jellyfin | The user your friend made for the link (not for a Rustyfin) |
| `password` / `password_file` | Jellyfin: one of them | The user's password, or a file holding it (a trailing newline is ignored) |
| `libraries` | at least one | `remote` (the friend's library) merges into `local` (yours); with no `local`, it gets a library of its own |

Prefer `password_file` with a secret mounted into the container (a Docker Compose secret, say) over writing the
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
synced and how long that took, how many titles only it brought and how many copies it holds, how fast its server
answers catalogue calls (typical and slowest of the last 100), and how many syncs and requests failed, with a **Sync
now** button. Times and failures count from when this server started. The same is
`GET /api/v1/links` for administrators; `POST /api/v1/links/{id}/sync` starts a sync ([Rustyfin API](rustyfin-api.md)).

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

## Collections

Each sync also reads the friend's collections (one light request, plus one for each collection that has no TMDB id)
and adds their films to yours, on the next scan as well:

- **By TMDB collection id.** A friend's film says which TMDB collection it is in, and joins your collection with that
  id. A collection you do not have comes across under your friend's name for it.
- **Your name wins.** When you and your friend name a collection differently, yours is kept.
- **Collections with no TMDB id** (ones your friend made by hand) come across by their films, under their name.
- **Your own films stay where your NFOs put them**: a friend's copy of a film you have never moves it to another
  collection.
- When your friend drops a film or a collection, it leaves yours at the next sync. Collections you made by hand are
  never touched. A Rustyfin friend's collections do not come across yet.

## Which copy plays

All copies of a film or episode, yours and your friends', are ranked by:

1. resolution (a wide film is judged by its width, so 1920×800 counts as 1080p);
2. dynamic range: Dolby Vision, then HDR10 and HDR10+, then HLG, then SDR;
3. video bitrate, weighed by how efficient the codec is (AV1 counts double, HEVC one and a half times, VP9 1.4 times,
   against H.264);
4. audio channels;
5. lossless audio over lossy.

The best copy is listed first and plays when an app does not pick one; on a tie, your own copy wins. With
`[playback] prefer_direct_play = true`, a copy the app can play as is comes before a better one that would have to be
converted. An app's version
menu lists every copy, the friend's named with the link's name. A copy the app can play neither as is nor converted
(the friend's server is down) is listed after every copy it can play.

## What your friend sees

One signed-in device named "Rustyfin link" for the link user, and one stream (or transcode) per title playing through
the link. Your users' names and what they watch stay on your server.

## Linking two Rustyfin servers

A Rustyfin needs no password: your server reaches your friend's through their forward-auth proxy, which says who it is,
like it does for people. Your friend lets that user link, and your server syncs from their change feed instead of
listing everything each time.

**Your friend's side.** The user your server arrives as is an ordinary user: the same one you use to sign in to their
server, not a special account. On that user's page in their admin (**Allow linking**), or with
`PATCH /api/v1/users/{id}/permissions` and `{"may_link": true}`, they let it link. It is off for everyone until they
turn it on. Your server sees what that user can see.

**Your side.** No `user` or password:

```toml
[[link]]
name = "Alex"
kind = "rustyfin"
url = "https://media.alex.example"

[[link.libraries]]
remote = "Films"
local = "Movies"
```

Your server has to arrive at their proxy as that user. Over Tailscale, a node owned by a person is that person; a
tagged node (the usual way to run a server) is named by an app capability your friend grants it in their tailnet policy
(see [Authentication](authentication.md#tagged-devices)).

Behind a proxy that takes OIDC tokens (oauth2-proxy and the like), your friend registers your server as a client at
their identity provider, and the provider names it as the user your server arrives as. Add `[link.oidc]`:

```toml
[link.oidc]
issuer = "https://id.alex.example"           # or token_url = "https://id.alex.example/token"
client_id = "jonathans-rustyfin"
client_secret_file = "/run/secrets/alex-link-client-secret"
scope = "rustyfin"                            # if their provider wants one
```

Your server fetches a token with the OAuth 2.0 client-credentials grant (the id and secret by HTTP Basic), sends it as
`Authorization: Bearer` on every request, reuses it until a minute before it expires, and fetches a new one if the proxy
refuses it. It never checks a token; the proxy does. A refused secret shows in the Links page as "the token endpoint
... answered 401".

**The change feed.** `GET /api/v1/catalog` lists what changed in those libraries since a cursor: changed items (id,
type, library) and removed ones; without `since`, everything. Pages of up to `limit` items (1000 by default) say `more`
and give a `next` to pass as `after`; the first page's `cursor` is the `since` for next time. Details come from
`/Items?Ids=` as usual. A user not allowed to link gets 403, and the sync says so. Only your friend's own items are
offered, never copies they hold from another link. The first sync takes everything; after that a quiet sync is one
call, and a busy one fetches only what changed. If the friend refuses the cursor, the next sync starts over.

## When a friend's server is down

When a sync cannot reach your friend's server (or it refuses the link), or a stream through it fails and it does not
answer, the link is marked down:

- Titles only that friend has leave every view: lists, search, Latest, Continue Watching, Next Up, and a library made
  for one of their libraries. The friend's copy of a title you hold leaves its versions. A title another friend also
  holds stays, with that friend's copy standing in.
- Users who can see those titles get a message in their apps ("Alex's server can't be reached. Its titles are hidden
  until it is back."), the activity log says so, and the Links page shows the link as **Offline**.
- Nothing is deleted: watched state, resume points and playlists stay. A link that is down is tried every minute; when
  it answers, its titles come back and users get a second message.

## Who sees linked titles

Only people using your server. A server that links yours sees your own media and nothing you hold from your links: not
the titles, not the extra copies of titles you have too, not the libraries made for a friend's library. So when you and
a friend link each other, neither of you gets your own titles handed back, and a friend of a friend sees nothing they
were not given.

Rustyfin treats a request as a linking server's when its user may link (**Allow linking** on the user's page) or it
comes from Rustyfin's own link client. Give each server that links yours its own user, with **Allow linking** on; a
person's account should never have it. The admin's users list marks such accounts **Server**.

## Removing a link

Delete its `[[link]]` and restart. At start, Rustyfin removes every link the config no longer names: its copies, any
library of its own and what is in it, and the artwork your titles borrowed from it. The log says so:

```text
WARN  rustyfin_server: Link Alex is no longer in the config: removed it and its 412 copies
```

Watched history stays. What your users had of that friend's titles (watched, resume points, favourites, ratings) is
kept aside and given back when the same title turns up again, on your server or through another link, matched by its
provider ids (an episode by its show's ids and its season and episode numbers). A title another friend also holds stays
listed, with their copy. Your own titles stay, with any overview, genres or provider ids they borrowed. A link still
named in the config stays even if its settings are broken (a missing password file, say), so a typo never wipes it; fix
the setting and it syncs again.
