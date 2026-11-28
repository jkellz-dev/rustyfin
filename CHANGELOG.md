# Changelog

## [0.2.0] - 2026-11-28

### Bug Fixes

- *(media)* A transcode that finished the file keeps serving its segments
- *(media)* Count running encoders through a mutable borrow
- *(media)* Stop ffmpeg with SIGTERM, outside the manager's lock
- *(media)* Throttling reads ffmpeg's file-time progress; a segment request resumes a paused encoder
- *(search)* Reindex every item into Meilisearch, 10,000 at a time
- *(media)* Direct play answers suffix ranges and sends ETag and Last-Modified
- *(admin)* IPhone plays video in iOS's own player; no page scroll under the player
- *(media)* Clippy: name the timing cache's key, test-only position setter
- *(api)* The master playlist decides copy without the keyframe scan
- *(media)* Force encode keyframes at listed file times; serve init only once written
- *(media)* Ffmpeg's progress counts from the run's start
- *(media)* Clippy: a slice from a reference in the quality tests
- *(link)* Merge after scans, take every Jellyfin version, borrow what a local film lacks
- *(link)* Fetch every linked film again once, for its other versions
- *(link)* Clippy: name the row types of show placement
- *(admin)* Build a link's failure reason once, not in a moved closure
- *(link)* Images name their item entity_id, not item_id
- *(link)* Read the change feed's items, which are snake_case
- *(auth)* Pick the MediaBrowser header with an inline closure
- *(link)* Name the episode group member type, for clippy
- *(link)* Pass the state already borrowed, for clippy
- *(link)* Give each link task its own handle
- *(search)* Find collections, also when searching the Collections view
- *(users)* The root folder's libraries come in the user's order too
- *(link)* Remember a token's scope without expiry; forget when may_link changes
- *(parity)* Ancestors, extras and scheduled tasks answer as Jellyfin does
- *(config)* A [tasks] section without schedules keeps daily and weekly
- *(search)* Rebuild an index Meilisearch lost instead of updating it
- *(search)* Do not re-index the old index before swapping it out
- *(search)* Give the building index its settings again
- *(scan)* Stop re-probing and rewriting every track on each scan
- *(admin)* Show a 51-second track as 0:51, not 0m
- *(admin)* Work out the detail runtime label before the view moves the item
- *(people)* Keep a downloadable photo address; re-read titles stuck on local paths
- *(scan)* Look for a film's or episode's art on every scan
- *(scan)* Progress shares a library's folders; 100% means finishing
- *(items)* A library's id answers with its CollectionFolder
- *(items)* Use the existing distinct_tags; test helper takes a static query
- *(playback)* A distinct name for the problem-document 404 stub (E0428)
- *(parity)* One photo lookup for a page's cast; auth and audio level as decided
- *(search)* Rank a search first whatever the app sorts by
- *(search)* Index names without the marks inside their words
- *(auth)* The identity layer's future is Send
- *(api)* Write ids as Jellyfin does, 32 hex digits without dashes
- *(api)* Keep client and MusicBrainz ids as sent; tests read ids as Jellyfin writes them
- *(links)* A link's library shows its last sync; library counts are titles
- *(auth)* A new server's first user is its administrator again
- *(links)* A link's own library is changed only in rustyfin.toml
- *(music)* Genres of a music library are music genres
- *(music)* Genre pictures, and ID3 genre codes named
- *(music)* A genre id the cached names lack reads them again
- *(music)* GenreIds filters items and artists
- *(telemetry)* Rustyfin-api depends on anyhow
- *(music)* QueryBuilder::push takes the ORDER BY text as is
- *(music)* Genre and metric caches are per schema, not per database
- *(music)* A genre opens by id; request spans carry the path
- *(services)* Scan claims take ids and names, not borrowing closures
- *(services)* The provider-id check is a function, not a closure
- *(api)* The user image redirect reads the user through services
- *(services)* Remote control needs to be the session's user or an administrator
- *(api)* Drop the identity check's unused UserStore import
- *(services)* Next up and upcoming episodes respect library access
- *(server)* Drop the LDAP filter default left unused
- Keep Jellyfin's MediaStream apart from the stored stream model
- *(services)* Import AutoSet where the scanner groups film series
- *(services)* The scanner's enrichment and the session cleanup share the scanner's data layer
- *(models)* ItemQuery keeps its own Default (a page of 50)
- *(api)* Drop the views' unused CollectionStore import
- The server and the tests name the Postgres data layer where a type needs it
- *(data)* Drop the search service's unused Meilisearch check
- *(jellyfin)* Drop the identity layer's unused data handle
- *(api)* The link-scope state no longer shadows its middleware
- *(images)* A resized logo keeps its transparency
- *(images)* Art named after the video is read
- *(images)* Logos refetched, and seasons and episodes name their show's art
- *(parity)* The gaps the parity run of 2026-11-26 found

### CI/CD

- The release PR's description lists the changes
- Release-plz skips rustyfin-bench
- Rustyfin-bench has its own tag name for release-plz
- Run unit and integration tests in one job with one cache
- Share mbx's compile cache through an R2 bucket
- Give mbx the job token for its linker download
- Let mbx keep what it downloads; release builds get their own namespace
- Clang for wild in the release container; lint gets its own namespace
- The Tests job caches in a fresh namespace, rustyfin-test
- Clippy every target and run every crate's tests
- Write the release branch outputs in one redirect
- *(parity)* Every request carries the proxy's identity headers
- *(release)* Release-plz skips rustyfin-data, newer than the last tag

### Documentation

- Generic instructions, nothing from one installation
- Performance numbers from mise run bench only
- *(roadmap)* 16b merged libraries across servers, modelled on Jellyswarrm
- *(roadmap)* 16b links work with current Jellyfin, Rustyfin gets a faster sync
- Cover server linking
- *(roadmap)* Phase 17a, a Synology deployment
- *(roadmap)* Rustyfin links use forward auth, no link key
- *(roadmap)* Known bug, searching in collections finds nothing
- *(link)* May_link is set through /api/v1, and the feed's keys are snake_case
- *(roadmap)* Investigate playing a linked copy straight from the friend's server
- *(roadmap)* Debug layers and client logs to discuss; the remaining routes
- Set Rustyfin up with Docker Compose and Tailscale
- Layered architecture rules in CLAUDE.md and the architecture guide
- Warn at the top of the README that Rustyfin is alpha software
- The layers as they now are
- The crate layout as it now is
- *(roadmap)* This week's work, and boxes that shipped earlier

### Features

- *(bench)* Rustyfin-bench and mise run bench
- *(bench)* Pod memory limit, ffprobe CPU, Postgres and ffprobe versions, rounded results
- *(bench)* --meilisearch starts a Meilisearch for the server and times its index
- *(bench)* --streams plays a film to concurrent clients, direct and transcoded
- *(media)* Copy H.264 the client plays into HLS, convert only the audio
- *(media)* Copy HEVC to clients that list it, HDR only where they show it
- *(admin)* Videos played directly use the browser's own controls
- *(media)* Transcodes list the whole film in a VOD playlist
- *(admin)* Transcodes play on the film's timeline with the browser's controls
- *(media)* The best copy of a film comes first and plays by default
- *(link)* Merge a linked Jellyfin's films into local libraries
- *(link)* TV shows, libraries of their own, home videos; a bad mapping skips itself
- *(link)* Transcode a friend's copy on the friend's server, relayed here
- *(link)* The admin's Links page: state, last sync, counts, sync now
- *(link)* A link taken out of the config is removed at start
- *(link)* May link, and the change feed a linked Rustyfin syncs from
- *(api)* Rustyfin's own routes match in any case, written in lower case
- *(link)* Link a Rustyfin through forward auth, synced from its change feed
- *(auth)* Name a tagged Tailscale node's user by an app capability
- *(link)* Fetch an OAuth 2.0 client-credentials token for the friend's proxy
- *(admin)* Allow or stop a user linking a server, on their page
- *(link)* A friend's titles leave the views while their server is down
- *(api)* Every route matches in any case
- *(link)* Keep watched history of a removed link; option to prefer a copy that plays as is
- *(users)* Apply saved app settings, as Jellyfin does
- *(link)* A server linking this one sees only this server's own media
- *(admin)* Mark server accounts with a Server badge
- *(search)* Meilisearch ranks, Postgres filters; the index follows changes
- *(search)* People and artists through Meilisearch too
- *(admin)* Show how long each task's last run and each library's last scan took
- *(link)* Metrics per link
- *(links)* Show each link's last sync time, response time and failures
- *(config)* Run_on_startup is off by default
- *(streaming)* /Items/{id}/File and /Items/{id}/Download
- *(music)* Instant mixes, track genres, genre items, route aliases
- *(names)* Name items carry an item's fields, as in Jellyfin
- *(items)* Fields Jellyfin sends on folders, music, streams and lists
- *(playback)* Universal audio streams the file; bitrate test and media stubs
- *(playlists)* Sharing, old play reports, live streams, display content
- *(settings)* Server settings, devices and API keys in Jellyfin's shapes
- *(auth)* Tailscale is the authentication; tokens only pick the session
- *(auth)* Behind the proxy, no token signs anyone in
- *(auth)* Jellyfin's authentication actions are no-ops
- *(api)* Jellyfin's older *arr notifications, image path and small routes
- *(links)* A friend's films join the collections here
- *(links)* Scanning a library syncs the links that fill it
- *(tasks)* Interrupted runs say so and a library scan starts again
- *(telemetry)* OpenTelemetry traces and JSON logs
- *(auth)* [**breaking**] Remove LDAPS password sign-in
- *(meta)* Rustyfin artwork fetches title logos
- Libraries are set in rustyfin.toml
- *(api)* Media managers' webhook, and API keys from rustyfin.toml

### Miscellaneous

- Name no download-automation apps

### Performance

- *(music)* Find an artist's albums and tracks as a set, by index
- *(music)* Folder totals stop at tracks and read each item once
- *(music)* Count every music genre in one query
- *(links,search)* Background queries by equality join and index
- *(music)* A track is re-read when its file or its album NFO's content changes
- Idle database time and scan phases for films and shows
- *(music)* Genre pages and pictures use an index on genres
- *(music)* Genre ids re-read the names at most every ten seconds
- *(music)* Recently played reads the played items first
- *(music)* Recently played, list counts and the genre list, from live traces
- *(music)* The genre list is built once per library, pages cut from it
- *(music)* Genre lists are built in the background at start
- *(genres)* A genre list's pictures and counts in one query each

### Refactoring

- *(api)* Rustyfin's own routes under /api/v1, apart from Jellyfin's
- *(items)* Name the cast entry type (clippy::type_complexity)
- *(data)* Rename rustyfin-db to rustyfin-data
- *(data)* Data-layer traits, with display preferences behind them
- *(data)* Media items reach callers as rustyfin_models::data::MediaItem
- *(data)* Every row type reaches callers as a plain data model
- *(data)* Finish the row-model switch for activity log and task results
- *(data)* A data-layer trait for every area, implemented for Postgres
- *(api)* Route handlers ask the data layer, not the pool
- *(api)* The handlers' helpers take the data layer too
- *(data)* Data-model imports name only what each area uses
- *(data)* Name the credit tuples people queries return
- *(api)* The API runs no SQL of its own
- *(data)* Item lists go through MediaItemStore::query
- *(core)* Scanner, links, sessions, auth and search use the data layer
- *(core)* The calls the first pass missed go through the data layer
- *(services)* Rename rustyfin-core to rustyfin-services
- *(services)* Playlist rules move into rustyfin-services
- *(services)* Library access and user data rules move into services
- *(services)* Playback reports and play history move into services
- *(services)* Library management rules move into services
- *(services)* Starting scans and library notifications move into services
- *(services)* User rules move into services
- *(services)* API keys and hand-made collections move into services
- *(services)* Access tokens move into services
- *(auth)* Remove the device authorization flow nothing used
- *(services)* One rule for accounts the proxy vouches for
- *(api)* The API crate no longer depends on the data crate or sqlx
- Auth, search and media no longer depend on the data crate
- *(services)* The scanner, enrichment, links, tasks and sessions are generic over the data traits
- *(services)* Browsing moves into services.browse()
- *(services)* Which libraries a user sees comes from services
- The API is generic over the data layer; services name no database
- *(data)* The search index moves into the data layer
- *(models)* Rustyfin-models names no database
- The HTTP crate is rustyfin-jellyfin
- *(services)* Sessions carry their own remote-control commands
- Choosing the play method moves into rustyfin-jellyfin
- *(services)* What a trusted proxy's headers mean is a service rule
- Jellyfin's request identity moves into rustyfin-jellyfin
- The server applies metrics, logs, CORS and tracing to the API
- Rustyfin's own API gets its crate, rustyfin-api
- Jellyfin's DTOs move into rustyfin-jellyfin
- The linked-copy relay moves into rustyfin-api

### Styling

- *(search)* Rustfmt the reindex test

### Testing

- *(media)* A finished transcode, a resumed seek and throttling (red)
- *(media)* Wait for the restarted encoder before reading its arguments
- *(media)* Stopping a running transcode is prompt (red)
- *(media)* Fake ffmpeg scripts are committed fixtures
- *(media)* Ffmpeg reports file time under -copyts (red)
- *(search)* The reindex reaches every item, page by page (red)
- *(bench)* Report the Meilisearch index (red)
- *(bench)* Concurrent streams: playlists, the player's clock, the report (red)
- *(media)* H.264 the client plays is copied, placed by the playlist's real segment lengths (red)
- *(media)* The refused-copy PlaybackInfo test sends a device profile
- *(media)* HEVC copy for clients that list it, HDR only where shown, fMP4 with hvc1 (red)
- *(media)* Direct play answers suffix ranges, 416, ETag, Last-Modified and If-Range (red)
- *(admin)* Full screen on iPhone is iOS's own player (red)
- *(admin)* Video played directly uses the browser's own controls (red)
- *(media)* Full-film HLS playlists, segment cuts that survive restarts (red)
- *(admin)* Every video uses the browser's own controls (red)
- *(media)* Ffmpeg's progress counts from the run's start (red)
- *(media)* The best copy of a film comes first and plays by default (red)
- *(link)* A linked Jellyfin library merges into the local one (red)
- *(link)* Scans merge at once, every Jellyfin version is a copy, local films borrow (red)
- *(link)* Libraries of their own, a bad mapping skips itself, TV shows merge (red)
- *(link)* A friend's copy is transcoded by the friend through this server (red)
- *(link)* The fake Jellyfin's token is one no client token contains
- *(link)* The admin sees each link and can sync it now (red)
- *(admin)* The menu's pages include Links
- *(link)* A link taken out of the config is removed at start (red)
- *(link)* An admin lets an ordinary user link; the change feed (red)
- *(link)* The catalogue tests share the helpers like the others
- *(api)* Rustyfin's own routes live under /api/v1, apart from Jellyfin's (red)
- *(api)* Every /api/v1 route is lower case; /api is free in Jellyfin
- *(api)* Rustyfin's own routes match in any case (red)
- *(api)* Find every route's path, also where rustfmt wrapped it
- *(link)* A Rustyfin links through forward auth and syncs only what changed (red)
- *(auth)* A tagged Tailscale node is named by its app capability (red)
- *(link)* A Rustyfin behind an OIDC proxy is reached with a client-credentials token (red)
- *(admin)* The user page says whether the user may link a server (red)
- *(link)* A friend's titles leave the views while their server is down (red)
- *(db)* Break the visible-items view too when simulating a failed database
- *(api)* Jellyfin's routes match in any case (red)
- *(api)* The route-case tests share the helpers like the others
- *(search)* Collections are found, also inside the Collections view (red)
- *(link)* Watched history outlives a removed link; prefer a copy that plays as is (red)
- *(link)* Give the local copy an audio track the client plays
- *(users)* Saved app settings act on views, Latest and playback (red)
- *(users)* The settings tests share the helpers like the others
- *(link)* A linking server never sees what this server links (red)
- *(admin)* Server accounts wear a Server badge (red)
- *(search)* Meilisearch ranks, Postgres filters; the index follows changes (red)
- *(search)* Drop the ignored live check, now covered by search_ranked_tests
- *(search)* People and artists are found with typos; the people index follows changes (red)
- *(admin)* Tasks and scans say how long they took (red)
- *(parity)* Ancestors, extras and the startup scan answer as Jellyfin does (red)
- *(config)* A [tasks] section without schedules keeps the built-in ones (red)
- *(search)* An index Meilisearch lost is rebuilt whole (red)
- *(link)* Each link has its own metrics (red)
- *(link)* A copy of a local film is not one of the link's titles
- *(scan)* A rescan of unchanged files changes no item (red)
- *(scan)* Show what a rescan changed in each item
- *(links)* The Links page says how each link is doing (red)
- *(query-count)* SQLx's pg_type lookup is not the route's query
- *(admin)* Runtimes under a minute, and tracks, read sensibly
- *(api)* One integration test binary instead of 87
- *(api)* Each test app gets its own image cache folder
- *(people)* A web photo replaces a Jellyfin-era local path
- *(scan)* Art written after a title's first scan appears on the next
- *(scan)* A scan reads 100% only while it finishes
- *(config)* A restart runs no scan unless asked
- *(items)* A library can be fetched by its id
- *(streaming)* A track plays from /Items/{id}/File and downloads
- *(parity)* Catalog every Jellyfin route and ask the readable ones
- *(parity)* Media with a genre, studio and people; record every area again
- *(music)* Instant mixes, genre items, track genres and route aliases
- *(names)* People, genres, studios and years answer like any item
- *(items)* The fields Jellyfin sends on folders, music, streams and lists
- *(playback)* Universal audio streams itself; Jellyfin's small media routes
- *(playlists)* Sharing, old PlayingItems reports, live streams, display content
- *(search)* A sorted search still puts the best match first
- *(search)* Names with marks inside words are found without them
- *(settings)* Server settings, devices and API keys in Jellyfin's shapes
- *(auth)* A request Tailscale vouches for is that user's, whatever the token
- *(auth)* Behind the proxy, a token alone signs nobody in
- *(auth)* Jellyfin's authentication actions change nothing
- *(auth)* The *arr webhook comes from inside with a server key
- *(api)* Ids are written as 32 hex digits, as in Jellyfin
- *(api)* A doc comment clippy reads as a list item
- *(api)* Four more comparisons with hyphenated ids
- *(api)* Negative id checks look for the id as answers write it
- *(api)* Jellyfin's older *arr notifications, image path and small routes
- *(links)* A link's own library says when it synced; cards count titles
- *(auth)* The first person the proxy names is the administrator
- *(links)* A link's own library is changed only in the config
- *(links)* A friend's films join the collections here
- *(links)* Scanning a library syncs the links that fill it
- *(music)* An artist page stays fast in a large library
- *(music)* The artist list stays fast in a large library
- *(music)* Genres of a music library are music genres
- *(tasks)* A run the server stopped is interrupted, not failed
- *(music)* Genre pictures, and ID3 genre codes named
- *(music)* The genre picture test uses a readable PNG
- *(music)* A rewritten album NFO does not re-read its tracks
- *(music)* A genre page lists only that genre
- *(telemetry)* JSON log lines carry their trace id; a scan's SQL stays out of its trace
- *(music)* Recently played pages across played and unplayed
- *(music)* A genre opens by id
- The integration tests hand the data layer to scanners, links, sessions and search
- *(media)* Embedded cover tests never share a cache folder
- *(api)* Only a session's owner or an administrator may control it
- *(api)* Next up and upcoming stay within the user's libraries
- *(api)* Latest fills up from the libraries a limited user may see
- *(images)* A transparent picture stays transparent when resized
- *(images)* Allow for resampling in the logo bar check
- *(libraries)* Borrow single folders as slices (clippy)
- *(parity)* The metadata editor is a known difference

### Build

- *(bench)* Clap env feature; lockfile
- Link x86_64 Linux builds with wild, pinned, in every environment
- Lock rustyfin-search's new rustyfin-models dependency

## [0.1.0] - 2026-11-17

### Bug Fixes

- Use prefix_separator to avoid k8s service env var collision
- *(api)* Map FK violations to 404; fix clippy 1.99 lints
- *(api)* Accept camelCase query keys like ASP.NET
- *(server)* Exit 10s after a shutdown begins even if connections stay open
- *(api)* Stop HLS session ids and segment names becoming filesystem paths
- *(api)* Enforce a user's library access on every route that takes an item id
- *(api)* Validate the HLS session id before the item access check
- *(auth)* One token check for HTTP and the WebSocket; close sockets whose session ends
- *(api)* A transcode session belongs to the user who started it
- *(api)* GET /System/Info requires authentication
- *(api)* Stop hiding database errors behind unwrap_or_default
- *(api)* Percent-encode request values echoed into playlists and redirects
- *(api)* Recursive=true on /Items reaches every level
- *(api)* Report the real total for a library-restricted /Items query
- *(api)* List items always carry UserData, image fields, VideoType and RunTimeTicks
- *(api)* Finish the list-item shape change
- *(api)* Pass the borrowed item list through in the show routes
- *(api)* An item with no images keeps its empty image collections
- *(api)* Items say where they sit and what they are, as Jellyfin does
- *(api)* One condition, not two, when naming a season's series
- *(scan)* Series and album folders split name and year, keep their ids, stay per library
- *(images)* A signed-in caller gets 404 for images in libraries they may not see
- *(scan,images)* Hidden files are not media; artwork is read by its contents
- *(admin)* Browse TV as Media > TV Shows > show > season > episode
- *(api)* GET/POST /UserItems/{id}/UserData, so Plezzy can mark items watched
- *(common)* Episode image discovery sits before the test module (clippy)
- *(images)* Artwork that is not a readable image is served as missing
- *(api)* A busy or unreachable database answers 503 within 5 s
- *(nfo)* Read Jellyfin-written NFOs' ids, cast order and photos
- *(admin)* Breadcrumbs above the current one are links again
- Episode order, collection progress, Continue Watching per library, collection links, search thumbnails
- *(api)* Clippy manual_contains in the resume type filter
- *(admin)* Search thumbnail class worked out outside the view closures
- *(common)* A rate limit of N per window allows N per window
- Batch 1 of the owner's app reports
- *(items)* Sort numbered items by Jellyfin's sort name
- *(admin)* One person_id per breadcrumb
- *(admin)* ActivityLogEntry is Default (QueryResult needs it)
- UserData on Latest, Similar, Suggestions and playlist items
- ArtistIds finds an artist's albums and tracks; honour Filters
- Plezy badges and Continue Watching
- Scan item counters take u64 as is (clippy); lock metrics crates
- AlbumIds lists only that album's tracks
- Latest groups tracks into their album and episodes into their show
- *(health)* No mutable borrow in a match guard
- *(admin)* Is_multiple_of (clippy)
- Album artists count their albums; albums, shows and seasons add up their runtime
- Runtime totals by item type, not the folder flag
- *(nfo)* Read Lidarr's mm:ss track durations; an odd one no longer drops the album NFO
- The alphabet bar jumps: NameStartsWith/NameLessThan on artists and items
- The WebSocket reads the token where HTTP requests do (Plezy sends ?ApiKey=)
- LibraryRow carries last_scanned_at
- Sessions from before a restart show their playback
- MediaSources on episodes and NextUp, numbered SortName fallback
- Untitled episodes named after their file; 404s as problem details
- Jellyfin shapes for Ancestors, Latest, Sessions, cultures, library options and tasks
- *(scan)* Join album tracks per chunk (the stream adapter broke Send)
- *(db)* Concurrent migrations may race on CREATE EXTENSION
- *(collections)* Only films join a collection, not their extras or versions
- *(common)* Write the collection name as a JSON-escaped TOML string
- *(common)* The empty-string fallback, as a raw string
- *(admin)* Menu button bars visible; page no wider than a desktop screen
- *(admin)* Recently added shows albums and shows, with their art
- *(admin)* Placeholder initial computed before the view takes the label
- *(meta)* Fall back to the original cover when the archive has no thumbnail yet
- Hand-made collections (no extra_data) stay listed; tests for the two-film rule
- *(admin)* Move-safe links in the reactive views
- *(admin)* The media page takes its address from the router
- *(admin)* Favicon from any depth of address
- *(admin)* Recently added titles are cut to the tile's width
- Playback marks episodes played at the credits (Jellyfin's rules)
- One branch for played at the end or too short to resume (clippy)
- Readable client names; episodes in streams show show and number
- *(auth)* Keep DeviceId as sent; names are decoded
- *(admin)* Stacked card cells keep the column name in its own column
- *(admin)* Filters only for films and shows; the toolbar stays when nothing matches
- *(admin)* A toast timer never touches a page that was left
- *(admin)* Bind the route before matching on it (clippy)
- *(admin)* Say the page's name once it is drawn
- *(db)* Keep added_since above the test module (clippy)
- Music direct-plays when the client lists its format; a missing file is 404
- *(admin)* Clone what the player and detail views keep
- *(admin)* Ask the browser what it plays; fall back to a transcode
- *(admin)* Say so when a stream cannot be played
- Songs transcode through the audio playlist
- Remux only when the transcoding profile takes the source codecs
- Number subtitle files with a range; name the test's summary type (clippy)
- HLS playlists pass the api_key on to their links
- *(admin)* Play transcodes with hls.js wherever MediaSource exists

### CI/CD

- Release only on semver tag push, not every main push
- Release only on semver tag push, annotate tags with release notes
- Tag main pushes as :latest, semver tags get versioned tag only
- Package migrations directory in server container
- Pin actions and scope permissions for zizmor
- Restore the legacy Cargo layout so the mbx cache is saved
- Give each cargo job its own mbx cache key
- Route every Rust build through the mbx cache
- Restore each job's own mbx cache
- Let unit and integration share one mbx cache
- Log Cargo's fingerprint decisions in the release builds, once
- Let mbx itself say why the release builds recompile on a warm restore
- Release builds carry mbx's object store, not a pruned target dir
- Build the Debian base layers only when they change
- Re-run 90064ea (runner linker crashed with a bus error)
- Free runner disk before the integration tests
- Free runner disk in the unit-test job too
- *(parity)* Nightly Hurl parity run against the published image, no Jellyfin needed
- *(parity)* Run hurl from its image (the .deb needs a libxml2 the runner lacks)
- *(parity)* Commit the server config (rustyfin.toml is gitignored)
- Release-plz release PR and release, git-cliff changelog

### Documentation

- Finish Phase 11b — update roadmap (432→462 tests, annotate blocked items)
- *(roadmap)* Add 11h, a Hurl parity suite against a real Jellyfin
- *(roadmap)* The metadata CLI may run as a writable sidecar; the server stays read-only
- *(roadmap)* Benchmark later, N+1 next, metadata sidecar and collection art
- *(roadmap)* N+1 queries fixed
- *(roadmap)* Login rate limiting done
- *(roadmap)* Collection art is one folder per TMDB collection id
- Artwork command, collection art, clearing the 30-day no-art list
- *(roadmap)* Tick what batches 1, 4 and 5 shipped
- *(roadmap)* 11h first slice is in (97 requests, 8 areas, all matching)
- *(roadmap)* TV and music collections, deferred (owner)
- *(roadmap)* 11j extras (owner: films with extras show up more than once)
- *(roadmap)* Phase 16 multi-server (a friend's server beside yours; linked libraries)
- *(roadmap)* 11k lyrics (served from .lrc/.txt/tags; LRCLIB fetch in the CLI)
- *(roadmap)* Fold 15e lyrics into 11k
- Client request ranking (11a); stress tools (11b/11c/11d)
- *(lyrics)* The lyrics Job
- *(roadmap)* 11j and 11k done; 11h fixtures and nightly; 11a 404s documented
- *(roadmap)* 11i closed
- *(roadmap)* 12h mobile layout for the admin
- Performance at 80,000 items (11b/11d)
- *(performance)* Music scan with lofty, 2.8x faster
- *(roadmap)* 12h phone layout, first pass
- *(roadmap)* Back button, streams, play history done
- *(roadmap)* Tick the website generator and hosting (rustyfin.org)
- *(roadmap)* Error notices cover 12b's error toasts
- Crate overviews, the admin as it is now, contributing, new routes
- Every setting in the configuration reference; fix the example's database
- Moving from Jellyfin
- Architecture as it is now
- Play-method rules, subtitle files and scan progress
- Getting started; commit the dev proxy's Caddyfile
- API compatibility guide
- Deployment guide
- How PlaybackInfo offers subtitles; the new subtitle addresses
- Doc comments on every public model, repository and auth item
- Describe LibraryRow by its fields
- Users.configuration and the subtitle mode numbers
- README and the license text
- The server makes no TMDB requests; the rustyfin tool does
- No library size claim in the README

### Features

- *(api)* Jellyfin library management endpoints
- *(api)* ThemeSongs, ThemeVideos, ThemeMedia and Ancestors item routes
- *(api)* System shutdown, restart and log routes; drain on SIGTERM
- LibraryOptions.Enabled = false takes a library out of scanning
- *(api)* GET /Items/Counts and GET /Library/MediaFolders
- *(api)* The fields Jellyfin always sends
- *(media)* Keep and serve the stream details Jellyfin clients read
- *(api)* Second parity slice
- *(api)* Fields=MediaSources and Fields=MediaStreams on lists
- *(scan)* Prune items whose files are gone
- *(images)* A placeholder primary image for items without artwork
- *(music)* Tracks, albums and artists carry Jellyfin's music fields
- *(server)* CORS is same-origin unless origins are allowlisted
- *(music)* A track's release date is its year, from its tag or its album
- *(tv)* Seasons have covers, their own or the show's
- *(meta)* Rustyfin artwork fills missing posters and season posters from TMDB
- *(meta)* Accept TMDB's API Read Access Token
- *(tv)* Episodes show their own thumb or a frame from the video
- *(people)* Headshots from one shared folder, fetched once per person
- *(movies)* Collections from the film series in each film's NFO
- *(meta)* Fetch TMDB headshots at 632 px
- *(people)* Key headshots by TMDB id, else a hash of the photo
- *(auth)* Throttle failed logins per name and per address
- *(api)* The user's root folder, as Jellyfin's UserRootFolder
- *(collections)* Key automatic collections by TMDB collection id
- *(collections)* Collection art on the cache volume, by TMDB id
- A Collections library, and a collection's films on its page
- Library-scoped search, artwork --collections-only, remember what TMDB lacks
- *(admin)* Sort a library, open a person's titles, badges and progress on cards
- *(dashboard)* Library cards, now playing, recently added, activity
- Per-title .rustyfin.toml settings
- *(dashboard)* System health — database, search, library and cache folders
- Prometheus metrics on their own listener
- *(health)* Cache sizes and per-library folders instead of free space
- *(music)* MusicBrainz ids from the files' tags; NextUp offers never-started shows
- *(admin)* Scan shows it is scanning, and says when it finished
- Lyrics from sidecar .lrc/.txt or tags, and empty media segments
- Extras (trailers, featurettes, ...) belong to their title
- HasLyrics from the scan; tag reads only for changed tracks
- *(cli)* Rustyfin lyrics fetches missing lyrics from LRCLIB
- *(admin)* A title's page lists its extras (11j)
- Theme songs (11j); lyrics files listed as Lyric streams (11k)
- Versions of a film are one film (Jellyfin's multi-version rule)
- *(meta)* Rustyfin artwork --check-collections (opt-in)
- *(admin)* Phone layout (12h)
- Albums without a cover file show their first track's embedded cover
- Collections of one film are not listed
- *(meta)* Rustyfin artwork -t music fetches album covers
- Play history and minutes per kind of media
- *(admin)* Back button, live streams, user page with play history
- *(admin)* Streams near the top of the dashboard; Other only when played
- *(admin)* Each play in a user's history shows its picture
- *(admin)* Devices per user with sign out, user list order and filters
- *(admin)* Readable errors with retry, library form checks, no double submits
- *(admin)* Tables become stacked cards and dialogs fill the screen on phones
- *(admin)* Library filters, list view, bulk actions, search by kind
- *(admin)* Requests belong to their page, browsing cache, skeletons, optimistic changes
- *(admin)* Keyboard, focus and screen-reader support
- Scan progress, scan outcome and library changes for clients and the admin
- *(admin)* The Scan button shows the percent; the finished toast counts unreadable files
- *(admin)* Play video and music in the admin
- *(admin)* Continue watching on the dashboard
- Subtitle files beside a video are found and served
- Subtitles say how to get them (DeliveryMethod, DeliveryUrl)
- Keep a user's app settings (POST /Users/{userId}/Configuration)

### Miscellaneous

- Lock toml's writer for rustyfin-common
- Lock rustix for rustyfin-api

### Performance

- *(images)* Cache images and admin assets, resize once, stream files
- *(api)* Next up, resume and playlist items no longer query per item
- *(api)* Artists, a show's episodes, years and collection art in fixed queries
- *(scan)* Probe new files eight at a time
- *(scan)* Ingest an album's tracks eight at a time
- *(db)* Index the browse sort key; one item type compared with =
- *(db)* Trigram index for name search (pg_trgm when allowed)
- *(scan)* Read audio facts with lofty instead of spawning ffprobe

### Refactoring

- Split database config into separate fields (host, port, user, password, name)

### Styling

- Apply hk formatters to frontend, shell and markdown

### Testing

- Add media encoding unit tests (probe, subtitles)
- Add remaining parity tests (users list, empty name create, library delete, websocket, auth)
- *(search)* Live Meilisearch check, ignored without MEILI_URL
- *(api)* Routes must hide items in libraries the user cannot access
- *(api)* A transcode session belongs to the user who started it
- *(api)* Make the session takeover test reach the code that kills the job
- *(api)* Sweep every route with no credentials
- *(api)* A database failure on a route's primary data must be a 500
- Library_options migration applies on a database that already holds libraries
- *(api)* /System/Info is for signed-in users, not anyone
- *(api)* Request values echoed into playlists and redirects must be encoded
- *(api)* Recursive=true on /Items must reach episodes, seasons and tracks
- *(parity)* A Hurl suite that compares Rustyfin with a real Jellyfin
- *(api)* GET /Items/Counts and GET /Library/MediaFolders
- *(api)* List items always carry UserData, image fields, VideoType and RunTimeTicks
- *(parity)* Record whole numbers as numbers and show every failing assert
- *(api)* The fields Jellyfin always sends
- *(api)* Drop a stray assert that looked for a key named after the label
- *(api)* Streams carry Profile, Level, CodecTag, TimeBase, RefFrames, NalLengthSize
- *(media)* The ffprobe fixture names the new stream fields
- *(api)* Where an item sits and what kind of thing it is, as Jellyfin tells it
- *(parity)* Second slice, 29 more requests, and a recorder that survives header-less answers
- *(api)* Second parity slice, and streams scanned before the details get probed again
- *(api)* Fields=MediaSources on a list fills every item's sources
- *(api)* Series and album folders split name and year, keep their ids, stay per library
- *(api)* Scans prune items whose files are gone, within safety rules
- *(api)* Items without artwork name and serve a placeholder primary image
- *(api)* Tracks, albums and artists carry Jellyfin's music fields
- *(api)* CORS is same-origin unless an origin is allowlisted
- A track's release date is its year, from its tag or its album
- Hidden files are not media, and artwork is recognised by its contents
- *(api)* Seasons have covers, their own or the show's
- Images and admin assets cache well and cost little to serve
- *(meta)* TMDB read access tokens are sent as bearer headers
- *(api)* User data can be read and updated through /UserItems/{id}/UserData
- Episodes show their own thumb or a frame, in landscape
- *(api)* Episodes without art name a frame, not the landscape placeholder
- Headshots come from one shared folder, one file per person
- *(api)* Reach headshot_file_name through rustyfin-core
- *(api)* Only the first film casts someone without a headshot
- *(api)* Collections fill in from the film series in movie NFOs
- *(meta)* TMDB headshots are fetched at 632 px, not the original
- Headshots are keyed by TMDB id, else a hash of the photo, never a name
- *(api)* List routes make the same number of queries for 2 items as for 6
- *(api)* Artists, a show's episodes, years and collection art scale flat too
- *(api)* The rest of the four new cases and the warm-up request
- *(api)* A failing query count names the statements that grew
- *(api)* Count only the measured request's own queries
- *(api)* Return the recorded statements directly (clippy let_and_return)
- *(api)* A new pool connection's setup is not the route's query
- *(api)* Password logins are throttled per name and per address
- *(scan)* Broken files do not stop a scan or fail a request
- *(api)* A busy database pool answers 503 quickly, then recovers
- *(api)* GET /Users/{id}/Items/Root returns the user's root folder
- *(nfo)* A movie NFO written by Jellyfin's NFO saver
- *(nfo)* A movie NFO written by Radarr's Kodi metadata
- *(collections)* Collections are keyed by their TMDB collection id
- *(nfo)* Show and episode NFOs written by Sonarr
- *(collections)* Collection art from the cache volume
- *(admin)* Every breadcrumb but the last is a link
- A Collections library, and a collection's films on its page
- Browsing bugs the owner hit in real apps
- *(common)* A rate limit of N per window allows N per window
- *(query-count)* Measure one request at a time, and again if nothing was seen
- Batch 1 of the owner's app reports
- *(search)* /Search/Hints keeps to the library named by ParentId
- *(parity)* Mixed SortName lists order numbered items like Jellyfin
- *(admin)* Library sort options, person pages, card badges and progress
- *(dashboard)* /System/Stats lists libraries with counts and last scan
- Per-title .rustyfin.toml settings
- Drop a useless conversion (clippy)
- *(dashboard)* System health in /System/Stats
- Latest carries UserData (Plezzy badges count seasons)
- Artist albums by ArtistIds, Filters=IsPlayed (Plezzy music)
- Plezy badges and Continue Watching
- Prometheus metrics on their own listener
- Drop an unused import (clippy)
- Elide lifetimes (clippy)
- AlbumIds lists only that album's tracks (Plezy)
- Latest groups tracks into albums, episodes into shows (Plezy)
- Health reports cache sizes and library folders, not free space
- *(parity)* Music cases from Plezy's requests; artist ChildCount, album runtime
- MusicBrainz ids from tags; NextUp offers never-started shows as Jellyfin does
- *(parity)* MusicBrainz-tagged synthetic music; cases re-recorded
- Album NFO track durations in mm:ss (Lidarr)
- The alphabet bar filters artists (NameStartsWith, NameLessThan)
- A socket finds its token wherever the client put it (Plezy: ApiKey)
- Libraries report RefreshStatus and LastScanned (admin scan feedback)
- Playback from a session older than a restart is seen
- Lyrics and media segments routes (Plezy, roadmap 11a)
- Episodes and NextUp honour Fields=MediaSources,SortName (Plezy)
- *(parity)* Plezy area from the owner's live session (roadmap 11a)
- Extras belong to their title, not listed as films (roadmap 11j)
- Untitled episodes named after the file; 404 body is problem details (parity)
- Tracks carry HasLyrics from the scan (11k)
- *(parity)* Extras area; top-level lists are held to their first item
- Jellyfin shapes for Ancestors, Latest, Sessions, cultures and library options (parity)
- Theme songs of films and shows (11j); a lyrics file is a Lyric stream (11k)
- Ancestors tests expect the library and root folder, as Jellyfin
- Lofty audio facts match ffprobe; versions of a film are one film
- A collection lists a film with versions once
- An opt-in TMDB collection check joins collections split by stale NFOs
- Albums without a cover file show their first track's embedded cover
- An album without art names its embedded cover, not the shared placeholder
- Collections of one film are not listed
- *(meta)* Album covers from the Cover Art Archive (tests, stubbed)
- *(admin)* Addresses for the media browser; user page stats and history (stubbed)
- Play history and minutes per kind of media
- Playback marks played past the credits (Jellyfin's play-state rules)
- Readable client names and episode numbers in streams
- Play history names the picture of each play
- Devices per user, signing a device out, relative times, user list order
- Leave the helper's own sign-in out of the device list check
- *(admin)* Readable request errors and library form checks
- *(admin)* Library filters and search kinds in the address, view mode, selection
- *(admin)* Answers for a left page are dropped; recent answers are reused
- *(admin)* Page names for screen readers, arrow keys in lists
- Scan progress, last scan outcome and scan events
- *(core)* Find the last progress event with rfind (clippy)
- The scan board is global, not part of AppState
- Music matches Audio profiles; a missing file streams as not found
- The shared helper's unused parts are fine in the new test
- *(admin)* Player decisions (stream choice, resume, shortcuts, queue, tracks, chapters, reports)
- *(admin)* The device profile lists only what the browser plays
- A song transcodes through the audio playlist
- Subtitle files beside a video, and SubRip served as WebVTT
- HLS playlists pass the api_key on to their links
- *(admin)* The player prefers hls.js over the browser's own HLS
- Subtitles carry DeliveryMethod and a DeliveryUrl that works
- A user's app settings are kept and returned

### Build

- Dev profile keeps line tables only
- Pin Rust to 1.100.0
- *(frontend)* Serve trunk on port 3080 without backend proxy
- Add hk as formatter and linter
- Format frontend HTML and CSS with oxfmt
- Drop two unused dependencies
- Drop the lockfile entries only axum-extra needed
- Drop the unused axum-extra workspace dependency
- Lock brotli and flate2 for the admin asset server
- Latest workflow actions; Dependabot config; meilisearch-sdk 0.33
- Every crate at its latest release (manifests)
- Sqlx 0.9 (audited dynamic SQL, QueryBuilder without a lifetime)
- Drop QueryBuilder lifetimes sqlx 0.9 no longer needs (clippy)
- Lofty 0.25 and reqwest 0.13 in rustyfin-common
- Sha2 0.11 and lofty 0.25 in rustyfin-media and rustyfin-auth
- Rand 0.10 (random and random_range live on RngExt)
- Lockfiles at the latest releases
- Release-plz configuration (one version, one tag, one changelog)

### Debug

- Log database connection parameters at startup

### Wip

- *(ci)* Route cargo through mr-boxington (untested)
- Flate2 for the admin asset server (lock follows)
- Brotli for the admin asset server (lock follows)

## [0.1.0-20260222-c52fbb8] - 2026-02-22

### Documentation

- Test suite completion design (Phase 11b)
- Update roadmap with Phase 11b test suite completion (361→432 tests)
- Update CHANGELOG.md

### Features

- Cast & crew portrait cards with headshot images, NFO-aware scan etag

### Testing

- Add people repo unit tests (10 tests)
- Add DB repo tests for media_items, user_data, images, libraries
- Add NFO edge case tests and query param parsing unit tests
- Add remaining parity tests and DTO serialization tests

## [0.1.0-20260222-2c2bbd5] - 2026-02-22

### Bug Fixes

- Unify env var prefix — meta CLI now uses RUSTYFIN__ like server
- *(ci)* Use --unreleased for release notes instead of --latest
- Collapse nested if into let-chain for clippy collapsible_if

### Documentation

- Update CHANGELOG.md

## [0.1.0-20260222-b9c1ac1] - 2026-02-22

### Bug Fixes

- *(ci)* Commit CHANGELOG.md and create annotated tags

### Documentation

- Update CHANGELOG.md

## [0.1.0-20260222-ec6c8f6] - 2026-02-22

### Bug Fixes

- Getting Meillisearch working
- Wire unused NFO fields into NfoMetadata and enrichment pipeline
- Prevent panic when rate limiter config has zero values
- Show scan feedback by fixing tracing filter and log levels
- Resolve clippy warnings (collapsible_if, too_many_arguments)
- Re-enrich existing NFOs instead of skipping them
- Split genre tags on delimiters (;/,) into separate genres
- Split genre tags on delimiters in server-side scanner
- Guard user_data filters and fix ORDER BY edge cases in ItemQuery
- Trim whitespace in comma/pipe delimited parsers
- Align DTO response shapes with Jellyfin parity
- *(ci)* Build frontend before lint/test jobs
- *(ci)* Stub frontend/dist instead of building WASM for lint/test
- *(ci)* Switch reqwest from native-tls to rustls-tls
- *(ci)* Remove GITHUB_REPO from git-cliff to skip API calls

### CI/CD

- GitHub Actions pipeline with lint, test, build, containers, and releases
- Parallelize container builds with matrix strategy

### Documentation

- Initial documentation
- Jellyfin parity
- Roadmap and parity documents
- Metadata roadmap
- Update roadmap for Phase 7 transcoding completion
- Phase 9 Client UX design and implementation plan
- Phase 10d rustyfin-meta design document
- Phase 10d rustyfin-meta implementation plan
- Update roadmap with Phase 10d progress and binary rename
- Renumber roadmap phases, move Stretch Goals to last
- Phase 10d completion design (music NFO, rate limiting, interactive CLI)
- Phase 10d remaining work design (music providers, fanart.tv, gap detection)
- Phase 10d remaining implementation plan (14 tasks)
- Add workflow guide for rustyfin CLI metadata fetching
- Update roadmap for music metadata pipeline progress
- Add Phase 13 — Admin UI Streaming to roadmap
- Add MusicBrainz cache, setup script, and k8s deployment design
- Add task scheduler & scheduled scans design
- Add task scheduler implementation plan (10 tasks)
- Update roadmap for media update notifications (Phase 10b)
- Add items query engine design (10f)
- Add items query engine implementation plan
- Update roadmap for items query engine (10f)
- Add session capabilities design
- Add session capabilities implementation plan
- Update roadmap for session capabilities
- Add Phase 10 completion design (maintenance tasks + remote control)
- Add Phase 10 completion implementation plan
- Phase 11a+11b design — parity audit & integration test suite
- Phase 11 implementation plan — parity audit & integration tests
- Update roadmap — Phase 11a+11b progress
- Add stub endpoint inventory to roadmap
- Add Jellyfin parity test translation tasks and stub re-evaluation to roadmap
- Fix MusicGenres roadmap — music-only genre filter, not simple alias
- Add Phase 11g implementation plan

### Features

- Library Scanner & Media Probing
- Client Compatibility implementation
- Metadata enrichment
- Auth Module — Forward Auth + LDAPS Bind
- Working frontend
- Media browser
- File browser
- Library access perm
- Getting music libraries working
- Api endpoint stubs
- Admin/user authorization
- *(transcode)* Core infrastructure and models
- *(transcode)* API endpoints and routing
- *(transcode)* Server wiring and session integration
- *(ratings)* Add likes field and set_rating repo function
- *(ratings)* Route handlers and likes field in all DTOs
- *(nextup)* Real GET /Shows/NextUp implementation
- Suggestions, similar items, and movie recommendations
- Browsable genres, studios, and persons endpoints
- Image resizing, playlists, and collections
- *(common)* Add music tag reading, NFO writing, and artwork download
- *(meta)* Rustyfin-meta CLI with scan and fetch commands
- Phase 10d — rate limiting, disambiguation, music NFO, provider IDs
- Enhance music scan to write artist.nfo and album.nfo from tags
- Extract MusicBrainz IDs, label, release type from audio tags
- Wire extended audio tag fields into music NFO writing
- Add artist enrichment fields to NfoMetadata and WriteArtistNfo
- Add MusicBrainz provider with artist/album lookup and Cover Art Archive
- Add MusicBrainz/Fanart/Discogs config and SearchResult MBID field
- Integrate MusicBrainz provider into music scan pipeline
- Add fanart.tv provider for supplementary artwork
- Integrate fanart.tv into movie, TV, and music scan pipelines
- Add Discogs provider with OAuth 1.0a authentication
- Integrate Discogs provider into music scan for artist images and label enrichment
- Add gap detection warnings during music scan
- Add self-hosted MusicBrainz Docker setup with setup script
- Add track listings to album.nfo (Rustyfin extension)
- Add MusicBrainz disk cache, ScanOpts refactor, and setup.sh improvements
- Add generic task scheduler with cron-based library scans
- Add API key CRUD endpoints
- Add POST /Library/Media/Updated webhook endpoint
- Auto-notify server after fetch completes
- Add ItemQuery dynamic query builder
- Wire ItemQuery builder into GET /Items handler
- Add ClientCapabilities to SessionManager
- Add POST /Sessions/Capabilities endpoints
- Use stored DeviceProfile as fallback in PlaybackInfo
- Add maintenance task config structs
- Implement 5 maintenance scheduled tasks
- Add SessionCommand types and broadcast channels
- Register maintenance tasks with scheduler
- Add remote control REST endpoints
- Upgrade WebSocket handler for remote control command delivery
- Add integration test harness with TestApp
- Add individual genre/studio/person endpoints
- Add missing stub endpoints for Jellyfin client parity
- Add entity_id migration, display_preferences + task_results tables
- Persist display preferences to DB
- Implement MusicGenres with music-only genre filter
- Implement Years endpoint with real DB queries
- Implement GET /Items/{itemId}/Images with real ImageInfo DTOs
- Serve entity images for persons, studios, and artists
- Implement GET /Shows/Upcoming with real episode query
- User avatar images via config URL template redirect
- Implement Scheduled Tasks REST API
- Admin UI polish — scheduled tasks page, stable SPA navigation, mise tasks, clippy fixes
- Add image_url and provider_person_id to NfoPerson + NFO read/write
- Extract TMDB/TVDB person IDs, profile images, and writers from credits
- CLI headshot downloads + DB people image columns + enrichment wiring
- Server enrichment creates image rows from people cache directory
- Set primary_image_tag on BaseItemPerson when headshot image exists
- Migrate config loading to config crate

### Miscellaneous

- Initial
- Remove unused reqwest and quick-xml deps from rustyfin-core
- Fix clippy warning and formatting
- Cleanup
- Fix clippy warnings and update roadmap for Phase 10 completion
- Rename fnox secret key for config crate env convention

### Refactoring

- Extract naming, nfo, tmdb, images to rustyfin-common
- Server scanner reads album metadata from NFOs only

### Styling

- Cargo fmt

### Testing

- Add auth enforcement integration tests
- Add core CRUD integration tests
- Add DTO shape and edge case tests
- Port Jellyfin test suite — 264 new unit+integration tests

### Rename

- Rustyfin-meta binary → rustyfin, server binary → rustyfin-server


