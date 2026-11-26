# Authentication

Rustyfin does not check passwords or keep them. Something in front of it says who each request is from, and Rustyfin
only decides what that person may do: whether they are an administrator, which libraries they see, whether their account
is turned off.

That something is **a sign-in proxy** (forward auth): every request reaches Rustyfin through a proxy that adds a header
naming the person. [Tailscale Serve](#tailscale) is the simplest, and the one [Getting Started](getting-started.md)
uses. It is the only way in: a sign-in the proxy names no one for is refused, whatever user name and password the app
sent. `[auth.forward_auth]` must be configured, or the server does not start.

## Sign-in proxy

### How it works

1. A request arrives through the proxy, which adds a header naming the person (`Tailscale-User-Login`, or `Remote-User`
   from Authelia, Authentik, oauth2-proxy and the like).
2. Rustyfin believes that header only when the request comes straight from an address in
   `[auth.forward_auth] trusted_proxies`. From anywhere else the header is ignored.
3. The person named is the user, on every request. The first time someone is named, Rustyfin makes them a user; the
   very first user of a new server is its administrator.
4. Jellyfin's apps still sign in and keep a token, as they would with Jellyfin. Rustyfin goes along with it: sign-in
   ignores the user name and password typed and signs the app in as the person the proxy names, and the token only says
   which of their devices a request is from. Signing out, changing a password and signing a device out change nothing.

So the proxy must be the only way to reach Rustyfin, and it must replace these headers on every request rather than pass
on a copy a client sent. List only the proxy's own address in `trusted_proxies`, never a whole network other machines
share.

```toml
[auth.forward_auth]
trusted_proxies = ["127.0.0.1/32"]     # where the proxy connects from
admin_group = "media-admins"           # optional: people in this group are administrators

[auth.forward_auth.headers]            # defaults shown
user = "Remote-User"                   # required: who the person is
name = "Remote-Name"
email = "Remote-Email"
groups = "Remote-Groups"               # comma-separated
```

### Tailscale

[Tailscale Serve](https://tailscale.com/kb/1312/serve) passes HTTPS requests from your tailnet on to Rustyfin and adds
`Tailscale-User-Login` and `Tailscale-User-Name`, naming the person whose device sent the request. It removes any copies
a request already had. Every device you sign in to Tailscale is a known person, so Jellyfin's apps on phones and TVs
work with nothing more to type, and the server is never on the internet.

The [Getting Started](getting-started.md) setup runs Tailscale in a container beside Rustyfin, sharing its network, with
Serve passing requests to `http://127.0.0.1:8096`:

```toml
[auth.forward_auth]
trusted_proxies = ["127.0.0.1/32"]

[auth.forward_auth.headers]
user = "Tailscale-User-Login"
name = "Tailscale-User-Name"
email = "Tailscale-User-Login"
```

With Tailscale on the host itself instead, `tailscale serve --bg 8096` does the same; keep Rustyfin's port closed to
everything but `127.0.0.1`.

Tailscale sends no groups, so `admin_group` does not apply: the first user is the administrator, and the admin's
**Users** page makes others administrators. People you share the server's machine with (Tailscale's **Share**) are
named too, and become users like anyone else.

### Tagged devices

A device added to your tailnet with a tag (a shared TV, or another server) belongs to no one, so Tailscale names nobody
for it and Rustyfin refuses it. To let it in as someone, grant it an app capability in your tailnet policy, with the
user it should be:

```json
"grants": [{
  "src": ["tag:living-room-tv"],
  "dst": ["tag:rustyfin"],
  "ip": ["tcp:443"],
  "app": {"example.com/cap/rustyfin": [{"user": "alice@example.com", "name": "Living room"}]}
}]
```

Have Serve pass the capability on (`tailscale serve --accept-app-caps example.com/cap/rustyfin ...`, or
`"AcceptAppCaps": ["example.com/cap/rustyfin"]` beside `"Proxy"` in `serve.json`) and name it in Rustyfin:

```toml
[auth.forward_auth]
app_capability = "example.com/cap/rustyfin"
```

A request from that device is then `alice@example.com`, the same user Alice is on her own devices. A request that names
a person is that person, whatever capability it carries. Only the tailnet's administrator can write grants. Another
Rustyfin [linking to this one](linked-servers.md) is let in the same way.

### Other proxies

Any reverse proxy that asks an identity provider about each request and passes the answer on in headers works: Caddy,
nginx or Traefik with Authelia, Authentik or oauth2-proxy. [Deployment](deployment.md#reverse-proxy) has examples.

Such a proxy usually signs people in with a login page in the browser. The admin and Jellyfin's web app can show it;
apps on TVs and phones cannot, and the proxy refuses their requests. If those apps matter, use Tailscale.

## Server API keys

An administrator makes an API key by sending `POST /Auth/Keys` with `{"Name": "Library notifier"}` (the admin has no
page for it yet); the answer's `Key` is shown only then. A key names no person and signs no one in: it is only for the
"a file changed" notifications other programs can send (`POST /Library/Media/Updated` and the older
`/Library/Movies/...` and `/Library/Series/...`), which they send with the key straight to Rustyfin, not through the
proxy.

## For developers

The `identity` layer (`crates/rustyfin-api/src/identity.rs`) runs before every route: a request from a trusted proxy
that names someone becomes that user's, in the request's extensions, with the session the token picks (or the device's,
or a new one). Routes take the user through extractors from `rustyfin-auth`:

- `AuthenticatedUser`: the user (from the identity layer, or a token where no proxy is configured); 401 without one.
- `AdminUser`: the same, and an administrator; 403 for anyone else.
- `OptionalUser`: the user if there is one (public routes that answer more to a signed-in user).
- `ApiKeyUser`: a server API key, for library notifications only.

A disabled user is refused everywhere. Tokens are accepted where Jellyfin's apps put them: the `Authorization:
MediaBrowser ... Token="..."` header (or `Emby`), `X-Emby-Token`, `X-MediaBrowser-Token`, and `api_key` or `ApiKey` in
the query.
