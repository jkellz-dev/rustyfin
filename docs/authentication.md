# Authentication

Rustyfin supports two authentication methods: forward auth (via reverse proxy headers) and LDAPS bind. Both are
independently configurable, and at least one must be enabled. Rustyfin intentionally does not support local passwords or
OIDC — all credential management is delegated to external systems.

## Overview

### Two Auth Methods

1. **Forward Auth**: Reverse proxy handles authentication and passes identity via HTTP headers
2. **LDAPS**: Direct LDAP bind authentication with service account lookup

### Configuration Requirements

- At least one method must be configured (forward auth or LDAPS)
- Multiple methods can be enabled simultaneously
- First user created is automatically granted admin privileges
- Admin privileges can also be auto-granted via group membership

### No Local Passwords

Rustyfin deliberately excludes:

- Local password database
- OIDC integration
- Password reset flows

All credential management happens outside Rustyfin. This reduces attack surface and centralizes authentication policy.

### Linked Servers

A [linked server](linked-servers.md) does not change how people sign in to Rustyfin. Rustyfin is the one signing in:
to a friend's Jellyfin, as the user the friend made for the link, with the password from `[[link]]` (or the file it
names). That user should see only the shared libraries; an administrator's API key is never needed.

A link between two Rustyfin servers adds no way in. It reaches the friend's Rustyfin through their forward-auth proxy,
as an ordinary user that the friend's admin allows to link. Rustyfin only ever *fetches* a credential for it (none over
Tailscale, an OAuth 2.0 client-credentials token from `[link.oidc]` otherwise) and never checks one.

## Forward Auth

### How It Works

Forward authentication delegates user authentication to a reverse proxy (Authelia, Authentik, oauth2-proxy, Tailscale,
etc.):

1. Reverse proxy authenticates the user (OIDC, SAML, etc.)
2. Proxy sets identity headers (username, email, groups)
3. Proxy forwards request to Rustyfin
4. Rustyfin reads headers and creates/updates user

### Trusted Proxy Requirement

**Critical security requirement**: Forward auth headers are only honored when requests originate from trusted proxy IP
addresses.

Why this matters: Without IP validation, any client could spoof headers and impersonate users. The `trusted_proxies`
list uses CIDR notation to specify which IPs are authorized to set identity headers.

### Default Headers

Rustyfin reads these headers by default (compatible with Authelia, Authentik, oauth2-proxy):

- `Remote-User`: Username (required)
- `Remote-Name`: Display name (optional)
- `Remote-Email`: Email address (optional)
- `Remote-Groups`: Comma-separated group list (optional)

All header names are configurable for non-standard proxy configurations.

### Admin Group Auto-Mapping

If `admin_group` is configured, users whose `Remote-Groups` header contains the specified group name are automatically
promoted to administrator.

Example: If `admin_group = "jellyfin-admin"` and a user authenticates with `Remote-Groups: jellyfin-admin,users`, they
become an admin.

### Security Model

Forward auth is only processed when:

1. Request IP matches a trusted proxy CIDR
2. The `Remote-User` header is present

If either condition fails, Rustyfin falls through to LDAPS authentication (if configured) or returns 401.

## LDAPS Authentication

### Overview

LDAPS authentication performs a two-step bind:

1. Service account binds and searches for the user DN
2. User credentials are verified by binding as the user

This approach allows Rustyfin to extract user attributes (email, display name, group membership) while verifying the
password.

### LDAPS Only (No STARTTLS)

Rustyfin only supports `ldaps://` URLs. STARTTLS is intentionally not supported because it allows downgrade attacks
where an attacker can intercept the connection before encryption negotiation.

Configuration URLs **must** start with `ldaps://`. The server will refuse to start otherwise.

### Flow

1. Connect to LDAP server over TLS (`ldaps://`)
2. Bind with service account (`bind_dn` + `bind_password`)
3. Search for user DN using `user_filter` with escaped username
4. Bind as the user with provided password
5. Extract attributes: `mail_attr`, `display_name_attr`, `memberOf`
6. Check admin group membership via `memberOf` attribute

### Username Filter Escaping

The `user_filter` template uses `{}` as a placeholder for the username. Before searching, Rustyfin escapes the username
per RFC 4515 to prevent LDAP injection attacks.

Example: User input `admin)(objectClass=*` becomes `admin\29\28objectClass=\2a`, preventing filter manipulation.

### Admin Group Mapping

If `admin_group_dn` is configured, Rustyfin checks the user's `memberOf` attribute. If any entry matches the configured
DN (case-insensitive), the user is promoted to administrator.

Example:

```toml
admin_group_dn = "cn=jellyfin-admins,ou=groups,dc=example,dc=com"
```

If the user's LDAP entry has:

```text
memberOf: cn=jellyfin-admins,ou=groups,dc=example,dc=com
```

They are granted admin privileges.

## Configuration Reference

### Forward Auth Configuration

Full TOML example:

```toml
[auth.forward_auth]
# Required: CIDR ranges of reverse proxies allowed to set identity headers
trusted_proxies = [
    "10.0.0.0/8",          # Private network
    "172.16.0.0/12",       # Docker bridge networks
    "127.0.0.1/32",        # Localhost
]

# Optional: Auto-admin users with this group
admin_group = "jellyfin-admin"

# Optional: Custom header names (defaults shown)
[auth.forward_auth.headers]
user = "Remote-User"
name = "Remote-Name"
email = "Remote-Email"
groups = "Remote-Groups"
```

Environment variable overrides:

```bash
# Comma-separated CIDR list
RUSTYFIN_TRUSTED_PROXIES="10.0.0.0/8,172.16.0.0/12"

# Custom headers
RUSTYFIN_FORWARD_AUTH_HEADER_USER="X-Forwarded-User"
RUSTYFIN_FORWARD_AUTH_HEADER_NAME="X-Forwarded-Preferred-Username"
RUSTYFIN_FORWARD_AUTH_HEADER_EMAIL="X-Forwarded-Email"
RUSTYFIN_FORWARD_AUTH_HEADER_GROUPS="X-Forwarded-Groups"
```

### LDAPS Configuration

Full TOML example:

```toml
[auth.ldap]
# Required: LDAPS URL (must use ldaps://)
url = "ldaps://ldap.example.com:636"

# Required: Service account for user searches
bind_dn = "cn=rustyfin,ou=services,dc=example,dc=com"
bind_password = "service-account-password"

# Required: Base DN for user searches
search_base = "ou=people,dc=example,dc=com"

# Optional: User filter with {} placeholder (default: "(uid={})")
user_filter = "(uid={})"

# Optional: Email attribute (default: "mail")
mail_attr = "mail"

# Optional: Display name attribute (default: "cn")
display_name_attr = "cn"

# Optional: Admin group DN (members auto-promoted to admin)
admin_group_dn = "cn=jellyfin-admins,ou=groups,dc=example,dc=com"
```

Environment variable overrides:

```bash
RUSTYFIN_LDAP_URL="ldaps://ldap.example.com:636"
RUSTYFIN_LDAP_BIND_DN="cn=rustyfin,ou=services,dc=example,dc=com"
RUSTYFIN_LDAP_BIND_PASSWORD="service-account-password"
RUSTYFIN_LDAP_SEARCH_BASE="ou=people,dc=example,dc=com"
RUSTYFIN_LDAP_USER_FILTER="(uid={})"
RUSTYFIN_LDAP_ADMIN_GROUP_DN="cn=jellyfin-admins,ou=groups,dc=example,dc=com"
```

## Setup Guides

### Kanidm (LDAPS)

Kanidm is a modern identity management system with built-in LDAPS support.

#### Kanidm Configuration

```toml
[auth.ldap]
url = "ldaps://idm.example.com:636"
bind_dn = "dn=token"
bind_password = "your-service-account-token"
search_base = "dc=idm,dc=example,dc=com"
user_filter = "(name={})"
display_name_attr = "displayname"
mail_attr = "mail"
admin_group_dn = "cn=jellyfin_admins,dc=idm,dc=example,dc=com"
```

Key differences from generic LDAP:

- `bind_dn = "dn=token"` — Kanidm uses token-based service accounts
- `user_filter = "(name={})"` — Kanidm uses `name` instead of `uid`
- `display_name_attr = "displayname"` — Kanidm attribute naming

#### Creating a Service Account in Kanidm

```bash
# Create service account
kanidm service-account create rustyfin "Rustyfin LDAP service account"

# Generate token
kanidm service-account api-token generate rustyfin "rustyfin-prod" --validity-days 365

# Grant search privileges
kanidm group add-members idm_people_read_priv rustyfin
```

Use the generated token as `bind_password`.

### LLDAP (LDAPS)

LLDAP is a lightweight LDAP server designed for self-hosting.

#### Docker Compose Example

```yaml
version: "3"
services:
  lldap:
    image: lldap/lldap:latest
    ports:
      - "3890:3890"   # Web UI
      - "6360:6360"   # LDAPS
    environment:
      LLDAP_JWT_SECRET: "your-random-secret"
      LLDAP_LDAP_USER_PASS: "admin-password"
      LLDAP_LDAP_BASE_DN: "dc=example,dc=com"
    volumes:
      - ./lldap_data:/data
```

#### LLDAP Configuration

```toml
[auth.ldap]
url = "ldaps://lldap.example.com:6360"
bind_dn = "uid=admin,ou=people,dc=example,dc=com"
bind_password = "admin-password"
search_base = "ou=people,dc=example,dc=com"
user_filter = "(uid={})"
mail_attr = "mail"
display_name_attr = "cn"
admin_group_dn = "cn=lldap_admin,ou=groups,dc=example,dc=com"
```

Notes:

- LLDAP admin DN is always `uid=admin,ou=people,dc=example,dc=com`
- User filter uses `(uid={})` for standard LDAP compatibility
- Admin group is `cn=lldap_admin,ou=groups,dc=example,dc=com` by default

### Authelia Forward Auth

Authelia is a popular authentication proxy supporting OIDC, LDAP, file-based auth.

#### Authelia Configuration

Configure Authelia to set headers on successful authentication:

```yaml
# authelia/configuration.yml
server:
  host: 0.0.0.0
  port: 9091

authentication_backend:
  ldap:
    url: ldap://lldap:3890
    base_dn: dc=example,dc=com
    username_attribute: uid
    users_filter: "(&({username_attribute}={input})(objectClass=person))"
    groups_filter: "(member={dn})"

access_control:
  default_policy: deny
  rules:
    - domain: jellyfin.example.com
      policy: one_factor
```

Nginx configuration with Authelia:

```nginx
location / {
    proxy_pass http://rustyfin:8096;

    # Authelia forward auth
    auth_request /authelia;
    auth_request_set $user $upstream_http_remote_user;
    auth_request_set $name $upstream_http_remote_name;
    auth_request_set $email $upstream_http_remote_email;
    auth_request_set $groups $upstream_http_remote_groups;

    # Pass headers to Rustyfin
    proxy_set_header Remote-User $user;
    proxy_set_header Remote-Name $name;
    proxy_set_header Remote-Email $email;
    proxy_set_header Remote-Groups $groups;
}

location /authelia {
    internal;
    proxy_pass http://authelia:9091/api/verify;
}
```

#### Rustyfin Configuration

```toml
[auth.forward_auth]
trusted_proxies = ["172.16.0.0/12"]  # Docker network
admin_group = "jellyfin-admin"
```

### Authentik Forward Auth

Authentik is a self-hosted identity provider with OIDC, SAML, LDAP, and forward auth support.

#### Authentik Setup

1. Create a Proxy Provider in Authentik:
   - Type: Forward auth (single application)
   - External host: `https://jellyfin.example.com`
   - Mode: Forward auth (single application)

2. Create an Application:
   - Name: Jellyfin
   - Slug: jellyfin
   - Provider: (select the proxy provider)

3. Configure Outpost:
   - Type: Proxy
   - Provider: (select the proxy provider)

#### Nginx Configuration

```nginx
location / {
    proxy_pass http://rustyfin:8096;

    # Authentik forward auth
    auth_request /outpost.goauthentik.io/auth/nginx;
    auth_request_set $user $upstream_http_x_authentik_username;
    auth_request_set $email $upstream_http_x_authentik_email;
    auth_request_set $groups $upstream_http_x_authentik_groups;

    proxy_set_header Remote-User $user;
    proxy_set_header Remote-Email $email;
    proxy_set_header Remote-Groups $groups;
}

location /outpost.goauthentik.io {
    proxy_pass http://authentik-proxy:9000;
}
```

#### Rustyfin Configuration

```toml
[auth.forward_auth]
trusted_proxies = ["172.16.0.0/12"]
admin_group = "Jellyfin Admins"

# Authentik uses X-Authentik-* headers
[auth.forward_auth.headers]
user = "X-Authentik-Username"
name = "X-Authentik-Name"
email = "X-Authentik-Email"
groups = "X-Authentik-Groups"
```

### Tailscale

Tailscale can act as a forward auth provider using its built-in identity system.

#### Tailscale Serve Configuration

```bash
# Enable Tailscale serve with authentication
tailscale serve --bg --set-path / http://127.0.0.1:8096

# Enable identity headers
tailscale serve --bg --set-header "Tailscale-User-Login: ${user.loginName}"
tailscale serve --bg --set-header "Tailscale-User-Name: ${user.displayName}"
```

Or configure via Kubernetes operator:

```yaml
apiVersion: tailscale.com/v1alpha1
kind: Ingress
metadata:
  name: rustyfin
spec:
  backend:
    service:
      name: rustyfin
      port:
        number: 8096
  tls:
    mode: https
  tailscale:
    funnel: false
    auth: true
    serveConfig:
      headers:
        Tailscale-User-Login: "${user.loginName}"
        Tailscale-User-Name: "${user.displayName}"
```

#### Rustyfin Configuration

```toml
[auth.forward_auth]
# Tailscale CGNAT ranges (all Tailscale nodes)
trusted_proxies = [
    "100.64.0.0/10",           # IPv4 CGNAT
    "fd7a:115c:a1e0::/48",     # IPv6 ULA
]

[auth.forward_auth.headers]
user = "Tailscale-User-Login"
name = "Tailscale-User-Name"
email = "Remote-Email"  # Tailscale doesn't provide email
groups = "Remote-Groups"  # Tailscale doesn't provide groups
```

Note: Tailscale headers use the identity from your Tailscale network. Admin promotion must be done manually or via a
separate group system.

#### Tagged nodes: app capabilities

A tagged node (a server, such as another Rustyfin [linking to this one](linked-servers.md)) carries no user login, so
Tailscale sends no `Tailscale-User-Login` for it. Name its user with an app capability instead. In the tailnet policy,
grant the node a capability whose value has `user` (and optionally `name`):

```json
"grants": [{
  "src": ["tag:rustyfin-jonathan"],
  "dst": ["tag:rustyfin"],
  "app": {"example.com/cap/rustyfin": [{"user": "jonathan@example.com", "name": "Jonathan's Rustyfin"}]}
}]
```

Have Tailscale Serve forward it (`tailscale serve --accept-app-caps example.com/cap/rustyfin ...`) and name it in
Rustyfin:

```toml
[auth.forward_auth]
app_capability = "example.com/cap/rustyfin"
```

A request with no user header but that grant signs in as `jonathan@example.com`, the same user Jonathan is when he
signs in himself; a request that has a user header is that user, whatever it was granted. Only the tailnet's admin can
write grants. The header is believed only from `trusted_proxies`, like the others.

### Combined Setups

#### Kanidm LDAPS + oauth2-proxy OIDC

Use Kanidm's OIDC for web clients (forward auth) and Kanidm's LDAPS for native clients (direct LDAP bind).

##### Kanidm OIDC Provider Setup

```bash
# Create OAuth2 client
kanidm system oauth2 create jellyfin "Jellyfin" https://jellyfin.example.com/oauth/callback

# Grant permissions
kanidm group add-members idm_people_read jellyfin
```

##### oauth2-proxy Configuration

```yaml
# oauth2-proxy.yaml
provider: oidc
oidc_issuer_url: https://idm.example.com/oauth2/openid/jellyfin
client_id: jellyfin
client_secret: your-client-secret
redirect_url: https://jellyfin.example.com/oauth2/callback
cookie_secret: random-secret
email_domains: ["*"]
pass_authorization_header: true
pass_access_token: true
set_authorization_header: true
set_xauthrequest: true
```

##### Nginx Configuration

```nginx
location / {
    proxy_pass http://rustyfin:8096;

    auth_request /oauth2/auth;
    auth_request_set $user $upstream_http_x_auth_request_email;
    auth_request_set $groups $upstream_http_x_auth_request_groups;

    proxy_set_header Remote-User $user;
    proxy_set_header Remote-Groups $groups;
}

location /oauth2/ {
    proxy_pass http://oauth2-proxy:4180;
}
```

##### Rustyfin Configuration

```toml
# Forward auth for web clients
[auth.forward_auth]
trusted_proxies = ["10.0.0.0/8"]
admin_group = "jellyfin_admins"

# LDAPS for native clients (iOS, Android, Apple TV)
[auth.ldap]
url = "ldaps://idm.example.com:636"
bind_dn = "dn=token"
bind_password = "kanidm-service-token"
search_base = "dc=idm,dc=example,dc=com"
user_filter = "(name={})"
display_name_attr = "displayname"
admin_group_dn = "cn=jellyfin_admins,dc=idm,dc=example,dc=com"
```

This setup allows:

- Web clients: SSO via oauth2-proxy (no password prompt)
- Native clients: Direct LDAPS authentication (password prompt)

#### Tailscale + LLDAP Hybrid

Use Tailscale for remote access and LLDAP for local network clients.

##### Rustyfin Configuration

```toml
# Forward auth for Tailscale clients
[auth.forward_auth]
trusted_proxies = ["100.64.0.0/10", "fd7a:115c:a1e0::/48"]

[auth.forward_auth.headers]
user = "Tailscale-User-Login"
name = "Tailscale-User-Name"

# LDAPS for local network clients
[auth.ldap]
url = "ldaps://lldap.local:6360"
bind_dn = "uid=admin,ou=people,dc=example,dc=com"
bind_password = "admin-password"
search_base = "ou=people,dc=example,dc=com"
user_filter = "(uid={})"
admin_group_dn = "cn=lldap_admin,ou=groups,dc=example,dc=com"
```

This allows:

- Tailscale users: Automatic login via Tailscale identity (no password)
- Local network users: LDAPS authentication with password

## Session Management

### Token Usage

After obtaining a token via `POST /Users/AuthenticateByName`, clients include it in subsequent requests. Rustyfin checks
these locations in order:

#### 1. Authorization Header (preferred)

```text
Authorization: MediaBrowser Client="Infuse", Device="Apple TV", DeviceId="abc123", Version="7.0", Token="a1b2c3d4e5f6"
```

Also accepts the legacy `Emby` prefix:

```text
Authorization: Emby Client="...", Token="..."
```

#### 2. X-Emby-Token Header

```text
X-Emby-Token: a1b2c3d4e5f6
```

#### 3. X-MediaBrowser-Token Header

```text
X-MediaBrowser-Token: a1b2c3d4e5f6
```

#### 4. Query Parameter

```text
GET /System/Info?api_key=a1b2c3d4e5f6
GET /System/Info?ApiKey=a1b2c3d4e5f6
```

### Session Lifecycle

- **Creation**: `POST /Users/AuthenticateByName` generates a random token and stores it in the `sessions` table
- **Validation**: Each authenticated request looks up the token, checks expiry, loads the user, and rejects disabled
  users
- **Activity tracking**: Each authenticated request updates `sessions.last_activity` and `users.last_activity_date`
- **Expiry**: Sessions have an optional `expires_at` timestamp. Expired sessions are rejected with 401.
- **Deletion**: When a user is deleted, their sessions are cascade-deleted via foreign key

### How Forward Auth Connects to Extractors

Forward auth and the Axum extractors operate at **different stages** of the request lifecycle — they never call each
other directly.

**Stage 1 — Login (`POST /Users/AuthenticateByName`):**

```text
Client → Reverse Proxy (Authelia/Authentik/Tailscale)
                │
                │ authenticates user, injects headers:
                │   Remote-User: alice
                │   Remote-Groups: admin,users
                │
                ▼
         Rustyfin auth.rs
                │
                ├─ Check: IP in trusted_proxies? ──── No ──→ Try LDAPS or 401
                ├─ Extract identity from headers
                ├─ Upsert user in DB (create on first login)
                ├─ Auto-promote if groups match admin_group
                ├─ Create session row → generate random token
                │
                └─ Return { access_token: "a1b2c3..." }
```

The client stores this token for all future requests.

**Stage 2 — Every subsequent request (`FromRequestParts`):**

```text
Client sends: Authorization: MediaBrowser ... Token="a1b2c3..."
                │
                ▼
     AuthenticatedUser::from_request_parts()
                │
                ├─ Parse token from header/query param
                ├─ Look up session in DB (sessions::find_by_token)
                ├─ Check expiry, load user, reject if disabled
                ├─ Touch session (update last_activity)
                │
                └─ Return AuthenticatedUser { user_id, is_administrator, ... }
```

At this point, **forward auth is completely irrelevant**. The extractor doesn't know or care how the user originally
logged in. Forward auth users and LDAP users look identical once they have a session token.

**Stage 3 — Admin escalation (`AdminUser`):**

`AdminUser` delegates to `AuthenticatedUser` first (all the token/session work), then adds a single check:

- `is_administrator == false` → **403 Forbidden** (not 401)

This layering produces distinct error codes: missing/invalid token → 401, valid token but not admin → 403.

**How admin status flows from forward auth:**

The admin bit is set during login based on group membership. When the reverse proxy sends `Remote-Groups: admin,users`
and `admin_group = "admin"` is configured, the login handler calls `users::update_policy(..., is_administrator: true)`.
All future `FromRequestParts` calls read that persisted flag from the database — the forward auth headers are never
re-checked after login.

**The generic state bound:**

The extractor is generic over any state type that provides a `PgPool`:

```rust
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
```

This means the extractor depends only on the database, not on `AppState` directly. It doesn't need access to config,
session managers, or search services. This separation keeps the auth layer decoupled from application concerns.

### Axum Extractors

#### `AuthenticatedUser`

Requires a valid, non-expired session token. Returns user info.

```rust
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub username: String,
    pub is_administrator: bool,
    pub session_id: Uuid,
    pub device_id: String,
}
```

Usage in route handlers:

```rust
pub async fn my_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,  // Request fails with 401 if not authenticated
) -> Result<Json<...>, ApiError> {
    // auth.user_id, auth.is_administrator, etc.
}
```

#### `AdminUser`

Requires a valid token AND `is_administrator = true`. Returns 403 Forbidden if the user is not an admin.

```rust
pub struct AdminUser(pub AuthenticatedUser);
```

Usage:

```rust
pub async fn admin_only_handler(
    State(state): State<AppState>,
    admin: AdminUser,  // 401 if not authenticated, 403 if not admin
) -> Result<StatusCode, ApiError> {
    let user = admin.0; // access the inner AuthenticatedUser
}
```

#### `OptionalUser`

Succeeds even without authentication. Useful for routes that behave differently for authenticated vs anonymous users.

```rust
pub struct OptionalUser(pub Option<AuthenticatedUser>);
```

## Security Notes

### Trusted Proxies Are Mandatory

Forward auth headers are **only** processed from IPs matching the `trusted_proxies` CIDR list. This prevents header
spoofing attacks where a malicious client sets `Remote-User: admin`.

Without IP validation, forward auth would be completely insecure. Always configure `trusted_proxies` to match only your
reverse proxy IPs, never `0.0.0.0/0`.

### LDAPS Only (No STARTTLS)

Rustyfin refuses `ldap://` URLs at startup. STARTTLS is not supported because it allows downgrade attacks where an
attacker intercepts the connection before TLS negotiation and captures credentials in plaintext.

Always use `ldaps://` URLs and ensure your LDAP server has a valid TLS certificate.

### LDAP Bind Password Security

The LDAP service account password is stored in plaintext in the configuration file. Use secure secrets management:

- Kubernetes: Use `envFrom` with a Secret
- Docker: Use Docker secrets or environment variables from a `.env` file (not checked into git)
- Systemd: Use `EnvironmentFile=` with restricted permissions

Example Docker Compose with secrets:

```yaml
services:
  rustyfin:
    image: rustyfin:latest
    environment:
      RUSTYFIN_LDAP_BIND_PASSWORD: ${LDAP_PASSWORD}
    env_file:
      - .env  # Not checked into git
```

### First User Auto-Admin

The first user to authenticate (via forward auth or LDAPS) is automatically promoted to administrator. This simplifies
initial setup but requires careful deployment:

1. Deploy Rustyfin with auth configured
2. Authenticate as your admin user first
3. Only then allow other users to access the system

If you accidentally grant admin to the wrong user, manually demote them via the admin UI or database.

### Disabled Users Are Rejected

Users with `is_disabled = true` are rejected at authentication time and at session validation. This applies to both
forward auth and LDAPS.

To disable a user:

1. Set `is_disabled = true` in the database or admin UI
2. Existing sessions are rejected immediately on next request

Disabled users cannot authenticate or use existing sessions.
