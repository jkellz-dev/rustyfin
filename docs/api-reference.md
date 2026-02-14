# API Reference

All endpoints return JSON with PascalCase field names. Authenticated endpoints require a valid token via one of the [supported auth methods](authentication.md).

## System

### `GET /System/Info/Public`

Server discovery. No authentication required.

**Response** `200 OK`:
```json
{
  "LocalAddress": "https://media.example.com",
  "ServerName": "Rustyfin",
  "Version": "0.1.0",
  "ProductName": "Rustyfin",
  "Id": "550e8400-e29b-41d4-a716-446655440000",
  "StartupWizardCompleted": true
}
```

`StartupWizardCompleted` is `false` when no libraries exist (triggers setup wizard in clients).

### `GET /System/Info`

Full system information. Requires authentication.

**Response** `200 OK`:
```json
{
  "LocalAddress": "https://media.example.com",
  "ServerName": "Rustyfin",
  "Version": "0.1.0",
  "ProductName": "Rustyfin",
  "Id": "...",
  "StartupWizardCompleted": true,
  "OperatingSystem": "linux",
  "OperatingSystemDisplayName": "linux x86_64",
  "HasPendingRestart": false,
  "IsShuttingDown": false,
  "HasUpdateAvailable": false,
  "SupportsLibraryMonitor": true,
  "CanSelfRestart": false,
  "CanLaunchWebBrowser": false,
  "WebSocketPortNumber": 8096,
  "ProgramDataPath": "",
  "ItemsByNamePath": "",
  "CachePath": "",
  "LogPath": "",
  "InternalMetadataPath": "",
  "TranscodingTempPath": ""
}
```

### `GET /System/Stats`

Dashboard statistics. Requires admin.

**Response** `200 OK`:
```json
{
  "LibraryCount": 3,
  "UserCount": 5,
  "ItemCount": 1247
}
```

### `GET /System/Ping` | `POST /System/Ping`

Health check. No authentication. Returns the server name as plain text.

---

## Startup

### `GET /Startup/Configuration`

Check if the setup wizard has been completed (i.e., libraries exist).

**Response** `200 OK`:
```json
{
  "StartupWizardCompleted": false
}
```

### `POST /Startup/Complete`

Mark startup as complete. No-op (library existence is the actual check).

**Response**: `204 No Content`

---

## Authentication

### `POST /Users/AuthenticateByName`

Create a session. Requires MediaBrowser header with device info.

**Headers**:
```
Authorization: MediaBrowser Client="My App", Device="Desktop", DeviceId="abc123", Version="1.0"
```

**Request Body**:
```json
{
  "Username": "john",
  "Pw": ""
}
```

The password field is ignored (forward auth handles authentication). The username is matched against forward auth headers or existing users.

**Response** `200 OK`:
```json
{
  "User": { "Name": "john", "Id": "...", "Policy": { "IsAdministrator": true, ... }, ... },
  "SessionInfo": { "Id": "...", "DeviceId": "abc123", ... },
  "AccessToken": "a1b2c3d4..."
}
```

---

## Users

### `GET /Users`

List all users. Requires authentication.

**Response** `200 OK`:
```json
[
  {
    "Name": "john",
    "ServerId": "...",
    "Id": "550e8400-...",
    "HasPassword": false,
    "HasConfiguredPassword": false,
    "HasConfiguredEasyPassword": false,
    "EnableAutoLogin": false,
    "LastLoginDate": "2025-01-15T10:30:00Z",
    "LastActivityDate": "2025-01-15T12:00:00Z",
    "Configuration": {
      "PlayDefaultAudioTrack": true,
      "SubtitleMode": "Default",
      "EnableNextEpisodeAutoPlay": true
    },
    "Policy": {
      "IsAdministrator": true,
      "IsHidden": false,
      "IsDisabled": false,
      "EnableAllFolders": true,
      "EnableMediaPlayback": true,
      "EnableAudioPlaybackTranscoding": true,
      "EnableVideoPlaybackTranscoding": true
    }
  }
]
```

### `GET /Users/Me`

Get current authenticated user.

### `GET /Users/Public`

Public user list for login screen. No authentication required.

### `GET /Users/{userId}`

Get a user by ID. Requires authentication.

### `DELETE /Users/{userId}`

Delete a user. Requires admin. Cannot delete yourself.

**Response**: `204 No Content`

**Errors**:
- `400` --- attempting to delete yourself
- `404` --- user not found

### `POST /Users/{userId}/Policy`

Update a user's admin policy. Requires admin.

**Request Body**:
```json
{
  "IsAdministrator": true
}
```

**Response**: `204 No Content`

---

## Libraries

### `GET /UserViews`

Get library root views. Requires authentication. Returns libraries as `CollectionFolder` items.

**Response** `200 OK`:
```json
{
  "Items": [
    {
      "Name": "Movies",
      "ServerId": "...",
      "Id": "...",
      "Type": "CollectionFolder",
      "CollectionType": "movies",
      "IsFolder": true
    }
  ],
  "TotalRecordCount": 1,
  "StartIndex": 0
}
```

### `GET /Users/{userId}/Views`

Legacy route. Same as `/UserViews`.

### `GET /Library/VirtualFolders`

List all libraries with their paths. Requires authentication.

**Response** `200 OK`:
```json
[
  {
    "Name": "Movies",
    "Locations": ["/media/movies"],
    "CollectionType": "movies",
    "ItemId": "550e8400-...",
    "PrimaryImageItemId": null,
    "LibraryOptions": {
      "EnablePhotos": false,
      "EnableRealtimeMonitor": false,
      "EnableChapterImageExtraction": false,
      "ExtractChapterImagesDuringLibraryScan": false,
      "AutomaticallyAddToCollection": false,
      "EnableInternetProviders": false,
      "SaveLocalMetadata": false
    }
  }
]
```

### `POST /Library/VirtualFolders`

Create a new library. Requires admin.

**Request Body**:
```json
{
  "Name": "Movies",
  "CollectionType": "movies",
  "Paths": ["/media/movies"]
}
```

**Response** `200 OK`: Returns the created `VirtualFolderInfo`.

### `DELETE /Library/VirtualFolders?Id={uuid}`

Delete a library. Requires admin.

**Response**: `204 No Content`

### `POST /Library/Refresh?Id={uuid}`

Trigger a library scan. Requires authentication. If `Id` is omitted, scans all libraries. The scan runs asynchronously in the background.

**Response**: `204 No Content`

---

## Items

### `GET /Items`

Query items with filters and pagination.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `ParentId` | UUID | Filter by parent item |
| `IncludeItemTypes` | String | Comma-separated types: `Movie`, `Series`, `Season`, `Episode` |
| `StartIndex` | i64 | Pagination offset (default 0) |
| `Limit` | i64 | Max results (default 100) |
| `SortBy` | String | Sort field |
| `SortOrder` | String | `Ascending` or `Descending` |
| `Recursive` | bool | Include nested items |
| `Fields` | String | Extra fields to include |

**Response** `200 OK`:
```json
{
  "Items": [
    {
      "Name": "Inception",
      "Id": "...",
      "Type": "Movie",
      "Overview": "A thief who steals corporate secrets...",
      "ProductionYear": 2010,
      "CommunityRating": 8.4,
      "OfficialRating": "PG-13",
      "RuntimeTicks": 88320000000,
      "Genres": ["Action", "Sci-Fi"],
      "ImageTags": { "Primary": "abc123" },
      "BackdropImageTags": ["def456"],
      "MediaSources": [...],
      "People": [...]
    }
  ],
  "TotalRecordCount": 42,
  "StartIndex": 0
}
```

### `GET /Items/Latest`

Get recently added items.

**Query Parameters**: `ParentId` (library UUID), `Limit`.

### `GET /Items/{itemId}`

Get full item detail with media sources, streams, people, and user data.

### `POST /Items/{itemId}/Refresh`

Re-enrich metadata for an item (re-runs NFO + TMDB lookup). Runs asynchronously.

**Response**: `204 No Content`

### `GET /UserItems/Resume`

Get items with in-progress playback for the current user.

### Legacy Routes

- `GET /Users/{userId}/Items` --- same as `GET /Items`
- `GET /Users/{userId}/Items/{itemId}` --- same as `GET /Items/{itemId}`

---

## Images

### `GET /Items/{itemId}/Images/{imageType}`

Serve an item's image. No authentication required.

**Path Parameters**:
- `imageType`: `Primary`, `Backdrop`, `Banner`, `Logo`, `Thumb`, `Art`, `Disc`

**Response**: Image bytes with appropriate `Content-Type` header.

### `GET /Items/{itemId}/Images/{imageType}/{imageIndex}`

Serve a specific image when multiple exist (e.g., multiple backdrops).

---

## Media Info

### `GET /Items/{itemId}/PlaybackInfo`

Get playback information for an item.

**Response** `200 OK`:
```json
{
  "MediaSources": [
    {
      "Id": "...",
      "Container": "mkv",
      "Size": 4294967296,
      "Bitrate": 8000000,
      "RuntimeTicks": 72000000000,
      "SupportsDirectPlay": true,
      "SupportsDirectStream": true,
      "SupportsTranscoding": false,
      "Type": "Default",
      "Protocol": "File",
      "MediaStreams": [
        {
          "Index": 0,
          "Type": "Video",
          "Codec": "hevc",
          "Width": 3840,
          "Height": 2160,
          "BitRate": 7500000,
          "IsDefault": true,
          "IsForced": false,
          "IsExternal": false
        },
        {
          "Index": 1,
          "Type": "Audio",
          "Codec": "aac",
          "Language": "eng",
          "Channels": 6,
          "ChannelLayout": "5.1",
          "IsDefault": true
        }
      ]
    }
  ]
}
```

### `POST /Items/{itemId}/PlaybackInfo`

Same as GET but accepts a device profile in the request body. The server compares the source media against the profile to decide direct play vs transcode.

**Request Body**:
```json
{
  "DeviceProfile": {
    "DirectPlayProfiles": [
      { "Container": "mp4,mkv", "VideoCodec": "h264", "AudioCodec": "aac,ac3", "Type": "Video" }
    ],
    "TranscodingProfiles": [
      { "Container": "ts", "VideoCodec": "h264", "AudioCodec": "aac", "Protocol": "hls", "Type": "Video" }
    ],
    "SubtitleProfiles": [
      { "Format": "vtt", "Method": "External" }
    ],
    "MaxStreamingBitrate": 20000000
  }
}
```

When transcoding is needed, the response includes:
```json
{
  "MediaSources": [{
    "SupportsTranscoding": true,
    "TranscodingUrl": "/Videos/{id}/master.m3u8?PlaySessionId=...&VideoCodec=h264&AudioCodec=aac&...",
    "TranscodingSubProtocol": "hls",
    "TranscodingContainer": "ts",
    "TranscodeReasons": ["VideoCodecNotSupported"]
  }]
}
```

---

## Streaming

### `GET /Videos/{itemId}/stream`

Direct play a video file. Supports HTTP Range requests for seeking.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `static` | bool | If `true`, always direct play |
| `MediaSourceId` | String | Media source identifier |
| `PlaySessionId` | String | Playback session identifier |

**Headers**:
- `Range: bytes=0-` --- standard HTTP Range header

**Response**: `200 OK` (full file) or `206 Partial Content` (range request) with:
- `Content-Type`: video MIME type
- `Content-Length`: byte count
- `Content-Range`: `bytes START-END/TOTAL` (for 206)
- `Accept-Ranges`: `bytes`

### `GET /Videos/{itemId}/stream.{container}`

Same as above with explicit container suffix (e.g., `stream.mp4`, `stream.mkv`).

### `HEAD /Videos/{itemId}/stream` | `HEAD /Videos/{itemId}/stream.{container}`

Stream metadata without body. Returns `Content-Type`, `Content-Length`, `Accept-Ranges` headers.

### `GET /Audio/{itemId}/stream` | `GET /Audio/{itemId}/stream.{container}`

Direct play an audio file. Same Range support as video.

---

## HLS Transcoding

### `GET /Videos/{itemId}/master.m3u8`

HLS master playlist for video transcoding. Requires authentication.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `MediaSourceId` | String | Media source identifier |
| `PlaySessionId` | String | Playback session identifier |
| `VideoCodec` | String | Target video codec (`h264`, `hevc`) |
| `AudioCodec` | String | Target audio codec (`aac`, `ac3`) |
| `MaxStreamingBitrate` | i64 | Max bitrate in bits/sec |
| `MaxWidth` | i32 | Max output width |
| `MaxHeight` | i32 | Max output height |
| `StartTimeTicks` | i64 | Seek position in 100ns ticks |
| `AudioStreamIndex` | i32 | Audio track index |
| `SubtitleStreamIndex` | i32 | Subtitle track index |
| `SubtitleMethod` | String | `Encode` (burn-in) or `External` |
| `SegmentContainer` | String | `ts` or `mp4` |

**Response** `200 OK` (`application/x-mpegURL`):
```
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=8000000,RESOLUTION=1920x1080,CODECS="avc1.640029,mp4a.40.2"
/Videos/{itemId}/main.m3u8?PlaySessionId=...&...
```

### `HEAD /Videos/{itemId}/master.m3u8`

Master playlist metadata without body.

### `GET /Videos/{itemId}/main.m3u8`

HLS variant playlist. Triggers ffmpeg if no transcode job is running. Returns segment list.

### `GET /Videos/{itemId}/hls1/{playSessionId}/{segmentFile}`

Serve an HLS segment. Waits for ffmpeg to produce the segment if not yet available.

**Response**: `200 OK` with `Content-Type: video/mp2t` or `video/mp4`.

### `DELETE /Videos/ActiveEncodings`

Kill an active transcode job.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `DeviceId` | String | Device to kill transcodes for |
| `PlaySessionId` | String | Specific session to kill |

**Response**: `204 No Content`

### Audio HLS

Audio equivalents of the above:

- `GET /Audio/{itemId}/master.m3u8` --- Audio HLS master playlist
- `GET /Audio/{itemId}/main.m3u8` --- Audio HLS variant playlist
- `GET /Audio/{itemId}/hls1/{playSessionId}/{segmentFile}` --- Audio segment

### `GET /Audio/{itemId}/universal`

Universal audio endpoint. Redirects to HLS transcoding or direct stream based on query parameters.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `Container` | String | Requested container format |
| `AudioCodec` | String | Requested audio codec |
| `TranscodingProtocol` | String | `hls` for HLS, otherwise direct stream |
| `MaxStreamingBitrate` | i64 | Max bitrate |

---

## Subtitles

### `GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.vtt`

Get a subtitle stream as WebVTT. External subs served directly; embedded subs extracted via ffmpeg.

### `GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.vtt`

Same with explicit media source ID.

### `GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.{format}`

Get a subtitle stream in the specified format (`vtt`, `srt`, `ass`).

---

## Sessions

### `GET /Sessions`

List active sessions. Requires authentication. Includes transcode status for active transcodes.

### `POST /Sessions/Logout`

End the current session.

**Response**: `204 No Content`

---

## Playback Reporting

### `POST /Sessions/Playing`

Report playback started.

**Request Body**:
```json
{
  "ItemId": "...",
  "MediaSourceId": "...",
  "PositionTicks": 0,
  "PlayMethod": "DirectPlay"
}
```

### `POST /Sessions/Playing/Progress`

Report playback progress (periodic updates).

**Request Body**:
```json
{
  "ItemId": "...",
  "PositionTicks": 36000000000,
  "IsPaused": false
}
```

### `POST /Sessions/Playing/Stopped`

Report playback stopped.

**Request Body**:
```json
{
  "ItemId": "...",
  "PositionTicks": 72000000000
}
```

### `POST /Sessions/Playing/Ping`

Keep-alive ping during playback.

**Response**: `200 OK`

---

## User Data

### `POST /UserFavoriteItems/{itemId}`

Mark an item as favorite. Returns updated `UserItemDataDto`.

### `DELETE /UserFavoriteItems/{itemId}`

Remove favorite mark.

### `POST /UserPlayedItems/{itemId}`

Mark an item as played. Returns updated `UserItemDataDto`.

### `DELETE /UserPlayedItems/{itemId}`

Mark an item as unplayed.

### Legacy Routes

All four operations also available at:
- `POST /Users/{userId}/FavoriteItems/{itemId}`
- `DELETE /Users/{userId}/FavoriteItems/{itemId}`
- `POST /Users/{userId}/PlayedItems/{itemId}`
- `DELETE /Users/{userId}/PlayedItems/{itemId}`

---

## Search

### `GET /Search/Hints`

Search for items by name.

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| `SearchTerm` | String | Search query (required) |
| `Limit` | i64 | Max results (default 20) |
| `IncludeItemTypes` | String | Filter by item types |

**Response** `200 OK`:
```json
{
  "SearchHints": [
    {
      "Id": "...",
      "Name": "Inception",
      "Type": "Movie",
      "ProductionYear": 2010,
      "PrimaryImageTag": "abc123"
    }
  ],
  "TotalRecordCount": 1
}
```

---

## WebSocket

### `GET /socket?api_key={token}`

WebSocket connection for real-time updates.

**Authentication**: Via `api_key` query parameter (validated against sessions table).

**Server Messages**:
```json
{
  "MessageType": "ForceKeepAlive",
  "Data": 60
}
```

Sent every 45 seconds. Client must respond within 60 seconds.

**Client Messages**:
```json
{
  "MessageType": "KeepAlive"
}
```

---

## Branding

### `GET /Branding/Configuration`

Returns branding options. No authentication.

**Response** `200 OK`:
```json
{
  "LoginDisclaimer": "",
  "CustomCss": "",
  "SplashscreenEnabled": false
}
```

### `GET /Branding/Css`

Returns custom CSS. No authentication. Returns empty string by default.

---

## Error Responses

All errors return a plain text body with the error message.

| Status | Meaning |
|--------|---------|
| `400` | Bad request (invalid input) |
| `401` | Not authenticated (missing/invalid token) |
| `403` | Not authorized (not admin for admin-only endpoints) |
| `404` | Resource not found |
| `500` | Internal server error |
