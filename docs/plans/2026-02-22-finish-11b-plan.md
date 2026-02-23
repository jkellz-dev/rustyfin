# Finish Phase 11b — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close out Phase 11b by adding ~15 remaining tests (parity + media encoding) and annotating roadmap items that are blocked on missing routes.

**Architecture:** Two independent test batches — integration parity tests in `parity_tests.rs` and inline unit tests in `probe.rs` + `subtitles.rs` — followed by a roadmap cleanup pass.

**Tech Stack:** Rust, sqlx, tower::ServiceExt, tokio, serde_json

---

### Task 1: Media encoding unit tests (~10 tests)

**Files:**
- Modify: `crates/rustyfin-media/src/probe.rs` (add `#[cfg(test)] mod tests` block)
- Modify: `crates/rustyfin-media/src/transcode/subtitles.rs` (add `#[cfg(test)] mod tests` block)

**Context:**

`probe.rs` has three pure functions with no tests:
- `seconds_to_ticks(seconds: f64) -> i64` — multiplies by 10_000_000
- `parse_frame_rate(s: &str) -> Option<f32>` — parses "N/D" fraction or plain float
- `map_stream(s: FfprobeStream) -> Option<ProbeStream>` — maps ffprobe JSON to domain struct

`subtitles.rs` has two pure functions with no tests:
- `subtitle_content_type(format: &str) -> &'static str` — format → MIME type
- `format_from_extension(ext: &str) -> &str` — file extension → canonical format

**Step 1: Add `probe.rs` test module**

Add at the end of `crates/rustyfin-media/src/probe.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── seconds_to_ticks ────────────────────────────────────

    #[test]
    fn seconds_to_ticks_zero() {
        assert_eq!(seconds_to_ticks(0.0), 0);
    }

    #[test]
    fn seconds_to_ticks_one_second() {
        assert_eq!(seconds_to_ticks(1.0), 10_000_000);
    }

    #[test]
    fn seconds_to_ticks_fractional() {
        // 1.5 seconds = 15_000_000 ticks
        assert_eq!(seconds_to_ticks(1.5), 15_000_000);
    }

    #[test]
    fn seconds_to_ticks_large_duration() {
        // 2 hours = 7200 seconds = 72_000_000_000 ticks
        assert_eq!(seconds_to_ticks(7200.0), 72_000_000_000);
    }

    // ── parse_frame_rate ────────────────────────────────────

    #[test]
    fn parse_frame_rate_fraction() {
        let rate = parse_frame_rate("24000/1001").unwrap();
        assert!((rate - 23.976).abs() < 0.01, "expected ~23.976, got {rate}");
    }

    #[test]
    fn parse_frame_rate_integer_fraction() {
        let rate = parse_frame_rate("30/1").unwrap();
        assert!((rate - 30.0).abs() < 0.01);
    }

    #[test]
    fn parse_frame_rate_plain_number() {
        let rate = parse_frame_rate("25").unwrap();
        assert!((rate - 25.0).abs() < 0.01);
    }

    #[test]
    fn parse_frame_rate_zero_denominator() {
        assert!(parse_frame_rate("30/0").is_none());
    }

    #[test]
    fn parse_frame_rate_invalid() {
        assert!(parse_frame_rate("abc").is_none());
    }

    // ── map_stream ──────────────────────────────────────────

    fn minimal_ffprobe_stream(codec_type: &str, codec_name: &str) -> FfprobeStream {
        FfprobeStream {
            index: 0,
            codec_type: Some(codec_type.to_string()),
            codec_name: Some(codec_name.to_string()),
            width: None,
            height: None,
            bit_rate: None,
            bits_per_raw_sample: None,
            pix_fmt: None,
            display_aspect_ratio: None,
            r_frame_rate: None,
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            channels: None,
            channel_layout: None,
            sample_rate: None,
            disposition: None,
            tags: None,
        }
    }

    #[test]
    fn map_stream_video() {
        let s = FfprobeStream {
            width: Some(1920),
            height: Some(1080),
            pix_fmt: Some("yuv420p".to_string()),
            r_frame_rate: Some("24000/1001".to_string()),
            ..minimal_ffprobe_stream("video", "h264")
        };
        let result = map_stream(s).unwrap();
        assert_eq!(result.stream_type, "Video");
        assert_eq!(result.codec.as_deref(), Some("h264"));
        assert_eq!(result.width, Some(1920));
        assert_eq!(result.height, Some(1080));
        assert!(result.channels.is_none(), "video streams should not have channels");
    }

    #[test]
    fn map_stream_audio() {
        let s = FfprobeStream {
            channels: Some(6),
            channel_layout: Some("5.1".to_string()),
            sample_rate: Some("48000".to_string()),
            ..minimal_ffprobe_stream("audio", "aac")
        };
        let result = map_stream(s).unwrap();
        assert_eq!(result.stream_type, "Audio");
        assert_eq!(result.channels, Some(6));
        assert_eq!(result.sample_rate, Some(48000));
        assert!(result.width.is_none(), "audio streams should not have width");
    }

    #[test]
    fn map_stream_subtitle() {
        let s = minimal_ffprobe_stream("subtitle", "subrip");
        let result = map_stream(s).unwrap();
        assert_eq!(result.stream_type, "Subtitle");
        assert_eq!(result.codec.as_deref(), Some("subrip"));
    }

    #[test]
    fn map_stream_unknown_type_returns_none() {
        let s = minimal_ffprobe_stream("attachment", "ttf");
        assert!(map_stream(s).is_none(), "attachment streams should be filtered out");
    }

    #[test]
    fn map_stream_missing_codec_type_returns_none() {
        let s = FfprobeStream {
            codec_type: None,
            ..minimal_ffprobe_stream("video", "h264")
        };
        assert!(map_stream(s).is_none());
    }
}
```

**Step 2: Add `subtitles.rs` test module**

Add at the end of `crates/rustyfin-media/src/transcode/subtitles.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── subtitle_content_type ───────────────────────────────

    #[test]
    fn content_type_vtt() {
        assert_eq!(subtitle_content_type("vtt"), "text/vtt; charset=utf-8");
    }

    #[test]
    fn content_type_webvtt_alias() {
        assert_eq!(subtitle_content_type("webvtt"), "text/vtt; charset=utf-8");
    }

    #[test]
    fn content_type_srt() {
        assert_eq!(subtitle_content_type("srt"), "text/plain; charset=utf-8");
    }

    #[test]
    fn content_type_ass() {
        assert_eq!(subtitle_content_type("ass"), "text/plain; charset=utf-8");
    }

    #[test]
    fn content_type_unknown_fallback() {
        assert_eq!(subtitle_content_type("xyz"), "text/plain; charset=utf-8");
    }

    // ── format_from_extension ───────────────────────────────

    #[test]
    fn extension_vtt() {
        assert_eq!(format_from_extension("vtt"), "vtt");
    }

    #[test]
    fn extension_srt() {
        assert_eq!(format_from_extension("srt"), "srt");
    }

    #[test]
    fn extension_ass() {
        assert_eq!(format_from_extension("ass"), "ass");
    }

    #[test]
    fn extension_ssa_maps_to_ass() {
        assert_eq!(format_from_extension("ssa"), "ass");
    }

    #[test]
    fn extension_case_insensitive() {
        assert_eq!(format_from_extension("VTT"), "vtt");
        assert_eq!(format_from_extension("SRT"), "srt");
    }

    #[test]
    fn extension_unknown_defaults_to_vtt() {
        assert_eq!(format_from_extension("xyz"), "vtt");
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p rustyfin-media`
Expected: All new tests pass alongside existing 25 command.rs tests.

**Step 4: Commit**

```bash
jj describe -m "test: add media encoding unit tests (probe, subtitles)"
jj new
```

---

### Task 2: Remaining parity tests (~5-7 tests)

**Files:**
- Modify: `crates/rustyfin-api/tests/parity_tests.rs`
- Reference: `crates/rustyfin-api/tests/common/mod.rs` (TestApp harness)

**Context:**

The TestApp harness provides `app.request(method, path)` returning a `tower::ServiceExt::oneshot` response. Auth is via `X-MediaBrowser-Token` header. Seed helpers: `create_user`, `create_library`, `create_media_item`.

Routes to test:
- `GET /Users` — exists in router.rs, returns user list
- `POST /Users/New` — exists, returns 403 (forward auth blocks creation)
- `DELETE /Library/VirtualFolders?name=<existing>` — exists, should succeed
- `GET /Items/{nonExistentId}/Similar` — route exists, should 404 or return empty
- `GET /socket` (WebSocket) without auth — should reject (401 or upgrade denied)

**Step 1: Add parity tests**

Append to the existing test file `crates/rustyfin-api/tests/parity_tests.rs`:

```rust
// ── User controller: remaining ──────────────────────────────

#[tokio::test]
async fn users_list_returns_seeded_users() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("alice", false).await;
    let resp = app
        .request(Method::GET, "/Users")
        .header("X-MediaBrowser-Token", &token)
        .send()
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await;
    let users = body.as_array().expect("response should be an array");
    assert!(
        users.iter().any(|u| u["Name"] == "alice"),
        "seeded user should appear in /Users list"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn users_new_with_empty_name_returns_403() {
    let app = common::TestApp::new().await;
    let (_user, token) = app.create_user("admin", true).await;
    let resp = app
        .request(Method::POST, "/Users/New")
        .header("X-MediaBrowser-Token", &token)
        .header("Content-Type", "application/json")
        .body(r#"{"Name": ""}"#)
        .send()
        .await;
    // Forward auth blocks all user creation — expect 403
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    app.cleanup().await;
}

// ── Library structure: delete by name ───────────────────────

#[tokio::test]
async fn delete_library_by_name_succeeds() {
    let app = common::TestApp::new().await;
    let (_user, token) = app.create_user("admin", true).await;
    let lib = app.create_library("ToDelete", "movies").await;
    let resp = app
        .request(
            Method::DELETE,
            &format!("/Library/VirtualFolders?name=ToDelete"),
        )
        .header("X-MediaBrowser-Token", &token)
        .send()
        .await;
    let status = resp.status();
    assert!(
        status == StatusCode::NO_CONTENT || status == StatusCode::OK,
        "expected 204 or 200, got {status}"
    );
    app.cleanup().await;
}

// ── Similar items: nonexistent item ─────────────────────────

#[tokio::test]
async fn similar_items_nonexistent_returns_empty_or_404() {
    let app = common::TestApp::new().await;
    let (_user, token) = app.create_user("alice", false).await;
    let fake_id = uuid::Uuid::new_v4();
    let resp = app
        .request(Method::GET, &format!("/Items/{fake_id}/Similar"))
        .header("X-MediaBrowser-Token", &token)
        .send()
        .await;
    let status = resp.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "expected 200 (empty) or 404, got {status}"
    );
    app.cleanup().await;
}

// ── WebSocket: no auth ──────────────────────────────────────

#[tokio::test]
async fn websocket_without_auth_rejected() {
    let app = common::TestApp::new().await;
    // Regular HTTP GET to /socket without WebSocket upgrade and without auth
    let resp = app
        .request(Method::GET, "/socket")
        .send()
        .await;
    let status = resp.status();
    // Without auth token, should get 401; without upgrade headers, may get 400
    assert!(
        status == StatusCode::UNAUTHORIZED
            || status == StatusCode::BAD_REQUEST
            || status == StatusCode::UPGRADE_REQUIRED,
        "expected 401/400/426 for unauthenticated non-WebSocket request to /socket, got {status}"
    );
    app.cleanup().await;
}
```

**Important:** The test code above is a guide. The implementer must check the actual TestApp API (how `request()`, `send()`, `json()` work) in `crates/rustyfin-api/tests/common/mod.rs` and adapt accordingly. The TestApp uses `tower::ServiceExt::oneshot`, not a real HTTP client — the request builder pattern may differ.

**Step 2: Run tests**

Run: `cargo test -p rustyfin-api`
Expected: All new parity tests pass alongside existing 62.

**Step 3: Commit**

```bash
jj describe -m "test: add remaining parity tests (users list, similar, websocket, library delete)"
jj new
```

---

### Task 3: Roadmap update + final verification

**Files:**
- Modify: `roadmap.md`

**Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass (should be ~450+).

**Step 2: Update roadmap**

Check off newly covered items. For items that can't be tested because routes don't exist, annotate them:

- Check off: `GET /Users` returns seeded users array
- Check off: `POST /Users/New` with invalid names
- Check off: WebSocket connect without auth
- Annotate blocked items with "— route not implemented" suffix
- Mark `DELETE /Items/{id}` items as N/A (server is read-only)
- Update test count in maturity summary
- Update media encoding tests checklist items

**Step 3: Commit**

```bash
jj describe -m "docs: update roadmap — finish Phase 11b test suite"
jj new
```
