# Authentication

Rustyfin uses **forward auth** exclusively. A reverse proxy (Authelia, Authentik, OAuth2-proxy, etc.) handles user authentication and passes identity information via HTTP headers. Rustyfin does not store passwords.

## Overview

```
Client → Reverse Proxy → [auth check] → Rustyfin
                                          ↓
                              Reads: Remote-User, Remote-Name, Remote-Email
                              Upserts user in DB
                              Creates session (access token)
                              ↓
                         Client stores token, uses for subsequent requests
```

## Forward Auth Headers

When a request passes through the reverse proxy, these headers are set:

| Header | Required | Description |
|--------|----------|-------------|
| `Remote-User` | Yes | Unique user identifier from the identity provider |
| `Remote-Name` | No | Display name |
| `Remote-Email` | No | Email address |
| `Remote-Groups` | No | Comma-separated group memberships |

The `Remote-User` value is stored as `remote_user_id` in the database and used to match returning users.

## Session Creation

Sessions are created via `POST /Users/AuthenticateByName`:

1. Client sends request with MediaBrowser header (device info) and username
2. Server checks for forward auth headers:
   - If `Remote-User` is present: upsert user via `remote_user_id`
   - If absent: look up user by username (fallback for dev/trusted networks)
3. Server creates a session record with a cryptographically random access token
4. Token is returned in the response

### First User

When the database has zero users and a new user is provisioned via forward auth, that user is automatically granted administrator privileges. Subsequent users are created as regular users.

## Token Usage

After obtaining a token, clients include it in subsequent requests. Rustyfin checks these locations in order:

### 1. Authorization Header (preferred)

```
Authorization: MediaBrowser Client="Infuse", Device="Apple TV", DeviceId="abc123", Version="7.0", Token="a1b2c3d4e5f6"
```

Also accepts the legacy `Emby` prefix:
```
Authorization: Emby Client="...", Token="..."
```

### 2. X-Emby-Token Header

```
X-Emby-Token: a1b2c3d4e5f6
```

### 3. X-MediaBrowser-Token Header

```
X-MediaBrowser-Token: a1b2c3d4e5f6
```

### 4. Query Parameter

```
GET /System/Info?api_key=a1b2c3d4e5f6
GET /System/Info?ApiKey=a1b2c3d4e5f6
```

## Axum Extractors

### `AuthenticatedUser`

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

### `AdminUser`

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

### `OptionalUser`

Succeeds even without authentication. Useful for routes that behave differently for authenticated vs anonymous users.

```rust
pub struct OptionalUser(pub Option<AuthenticatedUser>);
```

## Session Lifecycle

- **Creation**: `POST /Users/AuthenticateByName` generates a random token and stores it in the `sessions` table
- **Validation**: Each authenticated request looks up the token in `sessions`, checks expiry, loads the user
- **Activity tracking**: Each authenticated request updates `sessions.last_activity` and `users.last_activity_date`
- **Expiry**: Sessions have an optional `expires_at` timestamp. Expired sessions are rejected with 401.
- **Deletion**: When a user is deleted, their sessions are cascade-deleted via foreign key

## Reverse Proxy Configuration Examples

### Authelia

```yaml
access_control:
  default_policy: one_factor
  rules:
    - domain: media.example.com
      policy: one_factor
```

### Traefik + Authelia Middleware

```yaml
labels:
  - "traefik.http.routers.rustyfin.middlewares=authelia@docker"
```

### Caddy + Authelia

```
media.example.com {
    forward_auth authelia:9091 {
        uri /api/authz/forward-auth
        copy_headers Remote-User Remote-Name Remote-Email Remote-Groups
    }
    reverse_proxy rustyfin:8096
}
```

### nginx + Authelia

```nginx
location / {
    auth_request /authelia;
    auth_request_set $remote_user $upstream_http_remote_user;
    auth_request_set $remote_name $upstream_http_remote_name;
    auth_request_set $remote_email $upstream_http_remote_email;
    auth_request_set $remote_groups $upstream_http_remote_groups;
    proxy_set_header Remote-User $remote_user;
    proxy_set_header Remote-Name $remote_name;
    proxy_set_header Remote-Email $remote_email;
    proxy_set_header Remote-Groups $remote_groups;
    proxy_pass http://rustyfin:8096;
}
```

## Security Notes

- Rustyfin trusts `Remote-User` headers unconditionally. These headers **must** be set by the reverse proxy, never by the client directly. Ensure your reverse proxy strips any client-provided `Remote-*` headers before forwarding.
- Access tokens are 32-byte hex-encoded random values.
- There is no token rotation or refresh mechanism; sessions persist until deleted or expired.
- The WebSocket endpoint (`/socket`) authenticates via `?api_key=` query parameter since WebSocket connections cannot set custom headers.
