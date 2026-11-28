# Getting Started

Run Rustyfin with Docker Compose, add your libraries, and connect Jellyfin's apps. This takes four containers on one
machine: Rustyfin, Postgres for its data, Meilisearch for search, and Tailscale, which signs people in.

## How people sign in

Rustyfin has no passwords. Something in front of it says who each request is from, and Rustyfin decides what that person
may see. This guide uses [Tailscale](https://tailscale.com): every phone, TV and computer you sign in to your tailnet
is already a known person, so Jellyfin's apps connect with nothing more to type, and nothing is open to the internet.
Tailscale is free for personal use.

Another sign-in proxy can take Tailscale's place ([Authentication](authentication.md)), but apps on TVs and phones
cannot show a proxy's login page, so a proxy that asks for a password in the browser leaves them out.

## What you need

- A machine with Docker and Docker Compose, and your media folders on it.
- A Tailscale account. In its admin console:
  - **DNS**: turn on MagicDNS and **HTTPS Certificates**.
  - **Settings → Keys**: generate an auth key for the server. A key with a tag (for example `tag:rustyfin`) makes it a
    server rather than one of your own devices; the tag needs an entry in your policy's `tagOwners`.
  - **Access controls**: let the people who should watch reach the server on port 443. The default policy lets everyone
    in your tailnet reach everything.

## 1. The files

Make a folder with four files.

`.env`, the secrets (keep it out of version control):

```bash
TS_AUTHKEY=tskey-auth-...
POSTGRES_PASSWORD=a-long-random-password
MEILI_MASTER_KEY=another-long-random-string
```

`compose.yaml`:

```yaml
services:
  tailscale:
    image: tailscale/tailscale:latest
    # The server's name on your tailnet: https://rustyfin.<your-tailnet>.ts.net
    hostname: rustyfin
    environment:
      TS_AUTHKEY: ${TS_AUTHKEY}
      TS_STATE_DIR: /var/lib/tailscale
      TS_SERVE_CONFIG: /config/serve.json
    volumes:
      - tailscale:/var/lib/tailscale
      - ./serve.json:/config/serve.json:ro
    restart: unless-stopped

  rustyfin:
    image: ghcr.io/jkellz-dev/rustyfin-server:latest
    # Shares the tailscale container's network, so Tailscale reaches Rustyfin
    # at 127.0.0.1 and nothing else can claim to be its proxy.
    network_mode: service:tailscale
    environment:
      RUSTYFIN__DATABASE__PASSWORD: ${POSTGRES_PASSWORD}
      RUSTYFIN__SEARCH__MEILISEARCH_KEY: ${MEILI_MASTER_KEY}
    volumes:
      - ./rustyfin.toml:/app/rustyfin.toml:ro
      - /path/to/movies:/media/movies:ro
      - /path/to/tv:/media/tv:ro
      - /path/to/music:/media/music:ro
      - cache:/var/cache/rustyfin
    depends_on:
      - tailscale
      - postgres
      - meilisearch
    restart: unless-stopped

  postgres:
    image: postgres:17
    environment:
      POSTGRES_DB: rustyfin
      POSTGRES_USER: rustyfin
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    volumes:
      - postgres:/var/lib/postgresql/data
    restart: unless-stopped

  meilisearch:
    image: getmeili/meilisearch:v1.57
    environment:
      MEILI_MASTER_KEY: ${MEILI_MASTER_KEY}
      MEILI_NO_ANALYTICS: "true"
    volumes:
      - meilisearch:/meili_data
    restart: unless-stopped

volumes:
  tailscale:
  cache:
  postgres:
  meilisearch:
```

Change the `/path/to/...` lines to your folders. They are mounted read-only: Rustyfin never writes to your media.

`serve.json`, which tells Tailscale to pass HTTPS requests on to Rustyfin (keep `${TS_CERT_DOMAIN}` as it is;
Tailscale fills it in):

```json
{
  "TCP": { "443": { "HTTPS": true } },
  "Web": {
    "${TS_CERT_DOMAIN}:443": {
      "Handlers": { "/": { "Proxy": "http://127.0.0.1:8096" } }
    }
  }
}
```

Tailscale adds `Tailscale-User-Login` and `Tailscale-User-Name` to each request it passes on, naming the person who
sent it, and removes any copies the request already had.

`rustyfin.toml` (put your tailnet's name in `base_url`; every setting is in [Configuration](configuration.md)):

```toml
[server]
base_url = "https://rustyfin.your-tailnet.ts.net"
image_cache_dir = "/var/cache/rustyfin/images"

[database]
host = "postgres"

[search]
meilisearch_url = "http://meilisearch:7700"

[transcode]
temp_dir = "/var/cache/rustyfin/transcode"

# Believe who a request is from only when Tailscale passed it on.
[auth.forward_auth]
trusted_proxies = ["127.0.0.1/32"]

[auth.forward_auth.headers]
user = "Tailscale-User-Login"
name = "Tailscale-User-Name"
email = "Tailscale-User-Login"
```

## 2. Start it

```bash
docker compose up -d
docker compose logs -f rustyfin
```

Rustyfin sets up its database on the first start and says when it is listening. The first start of Tailscale also
fetches the HTTPS certificate, which can take a minute.

## 3. Open the admin

From a device on your tailnet, open `https://rustyfin.<your-tailnet>.ts.net/web`. You are signed in as your Tailscale
user. The first person to open it becomes the administrator; everyone after is an ordinary user. The admin's **Users**
page makes others administrators and chooses which libraries each person sees. Who can reach the server at all is up
to your tailnet: remove someone there to shut them out.

## 4. Add your libraries

**Libraries → Add Library**: a name, the type (Movies, Shows, Music or Mixed), and the folder as the container sees it
(`/media/movies`). The scan starts at once and the Scan button shows its progress; later scans run every night.

Rustyfin reads titles, plots, artwork and subtitles from the files beside your media, named the way Jellyfin and Kodi
name them ([Library Scanning](library-scanning.md)). It fetches nothing from the internet itself; to
fill in missing artwork or lyrics, see [Optional extras](#optional-extras).

## 5. Connect an app

In a Jellyfin app (Infuse, Swiftfin, Findroid, Finamp, Jellyfin Media Player, the Jellyfin TV apps, ...), on a device
signed in to your tailnet:

1. Add a server: `https://rustyfin.<your-tailnet>.ts.net`.
2. At the sign-in screen, type any user name and leave the password empty. Rustyfin signs the app in as the person
   Tailscale names for that device, whatever you type.

Signing out of an app changes nothing on the server; to stop a device, remove it from your tailnet.

A device added to your tailnet with a tag (a shared TV, say) belongs to no one, so Tailscale names nobody and Rustyfin
refuses it. Sign the device in to Tailscale as a person, or name a user for it with an app capability
([Authentication](authentication.md#tagged-devices)).

## Sharing with friends

Share the server's machine with them from Tailscale's admin console (**Machines → Share**). They join with their own
Tailscale account, which becomes their Rustyfin user; give them the libraries you want on the **Users** page.

## Keeping it running

- **Updating**: `docker compose pull && docker compose up -d`. The database is updated on start.
- **Backups**: the Postgres database holds users, watched state, play history and what the scans found:
  `docker compose exec postgres pg_dump -U rustyfin rustyfin > rustyfin.sql`. The cache and the search index are
  rebuilt by themselves; your media folders are untouched.
- **Health**: the admin's dashboard shows each library folder and the server's state. Logs:
  `docker compose logs rustyfin`.

## Optional extras

- **Artwork, cast photos and lyrics**: the `rustyfin` tool (image `ghcr.io/jkellz-dev/rustyfin-meta`) fetches what is
  missing from TMDB, MusicBrainz and LRCLIB. It writes artwork and lyrics beside your media, so it runs on its own with
  a writable mount, never inside the server ([Artwork](artwork.md), [Lyrics](lyrics.md)).
- **Hardware transcoding**: `[transcode] hardware_acceleration` (`qsv`, `vaapi` or `nvenc`), with the GPU passed into
  the `rustyfin` container (`devices: ["/dev/dri:/dev/dri"]` for Intel and AMD). Without it, ffmpeg converts on the CPU
  when an app cannot play a file as it is.
- **Metrics**: `[metrics] listen = "0.0.0.0:9090"` serves Prometheus metrics on a second port.
- **Library notifications**: a program that adds or changes files can tell Rustyfin which title changed, so it scans
  just that folder: media managers through their webhook ([Media webhook](rustyfin-api.md#media-webhook)), other
  programs with Jellyfin's `POST /Library/Media/Updated`. Put a key of at least 16 characters in a file, name it in
  `rustyfin.toml`, and add `[[path_map]]` if the program sees your media under other folders:

  ```toml
  [[api_key]]
  name = "notifier"
  key_file = "/run/secrets/notifier-key"
  ```

  The program sends the key as `X-Api-Key` (or `?api_key=`), straight to the `tailscale` container on port `8096` (run
  it in the same Compose file). The key only lets it send these notifications.
- **A friend's Jellyfin**: their libraries can merge into yours ([Linked Servers](linked-servers.md)).
- **Coming from Jellyfin**: what comes across and what does not ([Moving from Jellyfin](migrating-from-jellyfin.md)).
