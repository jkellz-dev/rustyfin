# Moving from Jellyfin

Rustyfin reads the same media folders Jellyfin does and answers the same apps, so a move is mostly pointing a new
server at your folders. What lives only in Jellyfin's database does not come across: there is no importer.

## What carries over

- **Your folders as they are.** The same naming rules (`Film (2020)/Film (2020).mkv`, `Show/Season 01/Show S01E01.mkv`,
  `Artist/Album/01 - Track.flac`), extras folders and `- trailer` files, and multiple versions of a film. See
  [Library Scanning](library-scanning.md).
- **NFO files** (Kodi format, as written by Radarr, Sonarr, tinyMediaManager or Jellyfin's own NFO saver): titles, plot,
  ratings, people, genres, studios, provider ids, and film collections from `<set>`.
- **Artwork beside the media**: `poster.jpg`, `folder.jpg`, `fanart.jpg`, `banner.jpg`, `logo.png`, `landscape.jpg`,
  season images, and art embedded in music files.
- **Subtitles**: inside the file, and files beside it named `<video name>.<language>[.forced|.sdh].srt` (also `.vtt`,
  `.ass`, `.ssa`), which is how Bazarr and Jellyfin name them.
- **Lyrics**: `.lrc`, `.elrc` or `.txt` beside a track, or in its tags. See [Lyrics](lyrics.md).
- **The apps.** Jellyfin apps (Infuse, Swiftfin, Jellyfin Media Player, ...) sign in to Rustyfin with its address, as
  they would to Jellyfin; testing each on real devices is still under way (roadmap 11a), and what clients ask for is
  tracked in [What clients ask for](client-requests.md). The Jellyfin web client can be pointed at Rustyfin from its
  own site (`server.cors_allowed_origins`); Rustyfin's own `/web` is its admin, which can also play.

## What does not carry over

- **Watched state, resume points, favourites and ratings.** They live in Jellyfin's database; everyone starts fresh.
- **Users and passwords.** Rustyfin signs people in through a forward-auth proxy (or an LDAPS server) and creates each
  user the first time they arrive. Library access and the administrator role are set again in the admin.
- **Metadata Jellyfin fetched but kept only in its database**, and artwork it downloaded into its own data folder.
  Rustyfin fetches its own from TMDB (`metadata.tmdb_api_key`), and the `rustyfin` tool fetches artwork and lyrics into
  the media folders (see [Artwork](artwork.md)).
- **Playlists and collections made in Jellyfin's apps.** Rustyfin has playlists, but they start empty; film
  collections come from NFO `<set>` and TMDB.
- **What Rustyfin leaves out on purpose**: plugins, Live TV and DVR, photo and book libraries, SyncPlay and DLNA.

## Before you switch

Two Jellyfin settings put more of what it knows into your folders, where Rustyfin reads it:

1. **Dashboard → Libraries → (each library) → Metadata savers: Nfo**, then refresh the library's metadata. Every title
   gets an NFO with Jellyfin's metadata and ids.
2. **Save artwork into media folders** (same page), then refresh, so posters and backdrops sit beside the files.

Both write to your media folders, so Jellyfin needs write access to them while it does this.

## Running both side by side

Rustyfin only reads the media folders, so both servers can point at the same ones. Both default to port 8096: give
Rustyfin another port (`server.port`) or another host name, add it in your apps as a second server, and compare.
Rustyfin keeps its own Postgres database and never touches Jellyfin's.

## Then

1. [Configure](configuration.md) Rustyfin: the database, the sign-in proxy, and a TMDB key.
2. Open `/web`, add the libraries with the same folders, and let the first scan run (the Scan button shows its
   progress).
3. Sign in from each app with the new address.
