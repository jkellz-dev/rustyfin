# Streaming & Media

Rustyfin supports **direct play** and **HLS transcoding**. Media files are served directly when the client supports the source format; otherwise, ffmpeg transcodes on-the-fly to HLS.

## Direct Play

### Video Streaming

```
GET /Videos/{itemId}/stream
```

### Audio Streaming

```
GET /Audio/{itemId}/stream
```

Both endpoints require authentication and serve the raw media file from disk.

## HTTP Range Support

Rustyfin fully supports [RFC 7233 Range Requests](https://tools.ietf.org/html/rfc7233), which enables:
- **Seeking** in media players (jumping to a specific timestamp)
- **Resuming** interrupted downloads
- **Parallel downloading** (multiple ranges)

### Request

```http
GET /Videos/{itemId}/stream HTTP/1.1
Authorization: MediaBrowser Token="..."
Range: bytes=0-1048575
```

### Response (Partial Content)

```http
HTTP/1.1 206 Partial Content
Content-Type: video/x-matroska
Content-Length: 1048576
Content-Range: bytes 0-1048575/4294967296
Accept-Ranges: bytes
```

### Supported Range Formats

| Range Header | Meaning |
|-------------|---------|
| `bytes=0-` | Entire file from start |
| `bytes=1000-` | From byte 1000 to end |
| `bytes=0-999` | First 1000 bytes |
| `bytes=-500` | Last 500 bytes |

### Full File Request (no Range header)

```http
HTTP/1.1 200 OK
Content-Type: video/mp4
Content-Length: 4294967296
Accept-Ranges: bytes
```

## Content Type Detection

MIME types are determined by file extension:

| Extension | Content-Type |
|-----------|-------------|
| `.mp4`, `.m4v` | `video/mp4` |
| `.mkv` | `video/x-matroska` |
| `.avi` | `video/x-msvideo` |
| `.mov` | `video/quicktime` |
| `.webm` | `video/webm` |
| `.mp3` | `audio/mpeg` |
| `.flac` | `audio/flac` |
| `.m4a` | `audio/mp4` |
| `.ogg` | `audio/ogg` |

## Media Probing

Media files are probed during library scanning using **ffprobe** (from the FFmpeg suite). ffprobe must be available on `$PATH`.

### Probe Command

```bash
ffprobe -v quiet -print_format json -show_format -show_streams <file_path>
```

### Extracted Information

#### Format (file-level)

| Field | Description | Stored As |
|-------|-------------|-----------|
| `duration` | Total duration in seconds | `runtime_ticks` (x 10,000,000) |
| `size` | File size in bytes | `size` |
| `bit_rate` | Overall bitrate | `total_bitrate` |
| `format_name` | Container format | `container` |

#### Video Streams

| Field | Description |
|-------|-------------|
| `codec_name` | Video codec (h264, hevc, av1, vp9) |
| `width` / `height` | Resolution |
| `bit_rate` | Video bitrate |
| `r_frame_rate` | Frame rate (e.g., "24000/1001") |
| `pix_fmt` | Pixel format (yuv420p, yuv420p10le) |
| `color_space` | Color space (bt709, bt2020nc) |
| `color_primaries` | Color primaries |
| `color_transfer` | Transfer characteristics (smpte2084 = HDR10) |
| `bits_per_raw_sample` | Bit depth (8, 10, 12) |

#### Audio Streams

| Field | Description |
|-------|-------------|
| `codec_name` | Audio codec (aac, ac3, eac3, dts, flac, opus) |
| `channels` | Channel count (2, 6, 8) |
| `channel_layout` | Layout (stereo, 5.1, 7.1) |
| `sample_rate` | Sample rate (44100, 48000) |
| `bit_rate` | Audio bitrate |
| `language` | Language code (eng, jpn, etc.) |

#### Subtitle Streams

| Field | Description |
|-------|-------------|
| `codec_name` | Format (srt, ass, subrip, hdmv_pgs_subtitle) |
| `language` | Language code |
| `disposition.forced` | Forced subtitle flag |
| `disposition.default` | Default track flag |

## Playback Info

### `GET /Items/{itemId}/PlaybackInfo`

Returns complete playback information for client decision-making:

```json
{
  "MediaSources": [
    {
      "Id": "item-uuid",
      "Container": "mkv",
      "Size": 4294967296,
      "Name": "Inception (2010).mkv",
      "Bitrate": 8000000,
      "RuntimeTicks": 88320000000,
      "SupportsDirectPlay": true,
      "SupportsDirectStream": true,
      "SupportsTranscoding": false,
      "IsRemote": false,
      "ReadAtNativeFramerate": false,
      "Type": "Default",
      "Protocol": "File",
      "ETag": "abc123",
      "MediaStreams": [
        {
          "Index": 0,
          "Type": "Video",
          "Codec": "hevc",
          "Language": null,
          "IsDefault": true,
          "IsForced": false,
          "IsExternal": false,
          "Width": 3840,
          "Height": 2160,
          "BitRate": 7500000,
          "BitDepth": 10,
          "PixelFormat": "yuv420p10le",
          "AspectRatio": "16:9",
          "RealFrameRate": 23.976,
          "ColorSpace": "bt2020nc",
          "ColorPrimaries": "bt2020",
          "ColorTransfer": "smpte2084"
        },
        {
          "Index": 1,
          "Type": "Audio",
          "Codec": "eac3",
          "Language": "eng",
          "Title": "English - Dolby Digital Plus 5.1",
          "IsDefault": true,
          "Channels": 6,
          "ChannelLayout": "5.1",
          "SampleRate": 48000,
          "BitRate": 640000
        },
        {
          "Index": 2,
          "Type": "Subtitle",
          "Codec": "srt",
          "Language": "eng",
          "Title": "English",
          "IsDefault": false,
          "IsForced": false,
          "IsExternal": true,
          "Path": "/media/movies/Inception (2010)/Inception.eng.srt"
        }
      ]
    }
  ]
}
```

## Playback Tracking

Clients report playback state to the server, which updates `user_data`:

### Start

```
POST /Sessions/Playing
{ "ItemId": "...", "PositionTicks": 0, "PlayMethod": "DirectPlay" }
```

### Progress

```
POST /Sessions/Playing/Progress
{ "ItemId": "...", "PositionTicks": 36000000000, "IsPaused": false }
```

Updates `user_data.playback_position_ticks` for resume support.

### Stop

```
POST /Sessions/Playing/Stopped
{ "ItemId": "...", "PositionTicks": 88320000000 }
```

If the position is near the end (>90%), marks the item as played.

### Ping

```
POST /Sessions/Playing/Ping
```

Keep-alive during active playback.

## User Data Tracking

### Favorites

```
POST   /UserFavoriteItems/{itemId}    → Mark favorite
DELETE /UserFavoriteItems/{itemId}    → Remove favorite
```

### Played Status

```
POST   /UserPlayedItems/{itemId}     → Mark as played
DELETE /UserPlayedItems/{itemId}     → Mark as unplayed
```

Both endpoints return the updated `UserItemDataDto`:

```json
{
  "PlaybackPositionTicks": 0,
  "PlayCount": 3,
  "IsFavorite": true,
  "Played": true,
  "LastPlayedDate": "2025-01-15T12:00:00Z"
}
```

## HLS Transcoding

When a client's device profile indicates it cannot direct-play the source file (incompatible codec, excessive bitrate, unsupported container), the server transcodes via ffmpeg into HLS segments.

### Decision Flow

1. Client sends `POST /Items/{itemId}/PlaybackInfo` with a `DeviceProfile` (lists supported containers, video/audio codecs, max bitrate)
2. Server compares source media streams against the profile
3. Result is one of:
   - **DirectPlay** — client plays the file as-is
   - **DirectStream** — remux (copy codecs, change container)
   - **Transcode** — re-encode video/audio to compatible codecs

The response includes `TranscodingUrl` (pointing to the HLS master playlist) when transcoding is needed.

### HLS Endpoints

```
GET  /Videos/{itemId}/master.m3u8    — Master playlist (codec/resolution info)
GET  /Videos/{itemId}/main.m3u8      — Variant playlist (triggers ffmpeg, lists segments)
GET  /Videos/{itemId}/hls1/{playSessionId}/{segment}.ts  — Individual segment
HEAD /Videos/{itemId}/master.m3u8    — Metadata only

DELETE /Videos/ActiveEncodings?deviceId=...&playSessionId=...  — Kill a transcode
```

Audio equivalents exist at `/Audio/{itemId}/master.m3u8`, `/Audio/{itemId}/main.m3u8`, etc.

### Universal Audio

```
GET /Audio/{itemId}/universal?container=...&audioCodec=...&transcodingProtocol=hls
```

Routes to either HLS transcoding or direct stream based on compatibility.

### Encoding

**Software (default):**
- Video: `libx264` (preset `veryfast`, CRF 23) or `libx265` (preset `fast`, CRF 28)
- Audio: `aac`, `ac3`, `libopus`, `libmp3lame`, or copy

**Hardware (optional, via config):**
- Intel QSV: `h264_qsv` / `hevc_qsv`
- VAAPI: `h264_vaapi` / `hevc_vaapi`
- NVENC: `h264_nvenc` / `hevc_nvenc`

Configure in `rustyfin.toml`:
```toml
[transcode]
ffmpeg_path = "ffmpeg"        # default: from PATH
hardware_acceleration = "qsv" # "none", "qsv", "vaapi", "nvenc"
max_concurrent = 4            # 0 = unlimited
segment_duration = 6          # seconds
threads_per_job = 0           # 0 = auto
```

### Throttling & Resource Management

- ffmpeg processes are paused (SIGSTOP) when encoding >120s ahead of playback
- Resumed (SIGCONT) when buffer drops below 30s or client requests a segment
- Stale jobs (no segment request in 60s) are automatically killed
- Background cleanup task runs every 30 seconds

### Seek Support

- Segments already transcoded are served from disk
- Seeking far ahead restarts ffmpeg with `-ss` at the target position
- Seeking backward serves existing segments

### Subtitle Delivery

```
GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.vtt
GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.vtt
GET /Videos/{itemId}/Subtitles/{streamIndex}/Stream.{format}
```

- External subtitle files are served directly
- Embedded subtitles are extracted via ffmpeg and cached on disk
- Image-based subtitles (PGS, VOBSUB) can be burned in during transcoding
