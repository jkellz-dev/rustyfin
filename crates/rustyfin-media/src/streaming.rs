use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Parse HTTP Range header: "bytes=START-END" or "bytes=START-".
fn parse_range(range_header: &str, file_size: u64) -> Option<(u64, u64)> {
    let range = range_header.strip_prefix("bytes=")?;
    let (start_str, end_str) = range.split_once('-')?;

    let start: u64 = if start_str.is_empty() {
        // Suffix range: "bytes=-500" means last 500 bytes
        let suffix: u64 = end_str.parse().ok()?;
        file_size.saturating_sub(suffix)
    } else {
        start_str.parse().ok()?
    };

    let end: u64 = if end_str.is_empty() {
        file_size - 1
    } else {
        end_str.parse().ok()?
    };

    if start > end || start >= file_size {
        return None;
    }

    Some((start, end.min(file_size - 1)))
}

/// Content type from file extension.
fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "ts" => "video/mp2t",
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "wma" => "audio/x-ms-wma",
        _ => "application/octet-stream",
    }
}

/// Serve a media file with HTTP Range support (direct play).
pub async fn serve_file(path: &str, headers: &HeaderMap) -> Result<Response, StatusCode> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let file_size = metadata.len();

    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let content_type = content_type_for_extension(ext);

    // Check for Range header
    if let Some(range_value) = headers.get(header::RANGE).and_then(|v| v.to_str().ok())
        && let Some((start, end)) = parse_range(range_value, file_size)
    {
        let length = end - start + 1;

        let mut file = File::open(path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let stream = tokio_util::io::ReaderStream::new(file.take(length));
        let body = Body::from_stream(stream);

        return Ok(Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, length.to_string())
            .header(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{file_size}"),
            )
            .header(header::ACCEPT_RANGES, "bytes")
            .body(body)
            .unwrap());
    }

    // Full file response
    let file = File::open(path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .body(body)
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_as_rfc_9110_says() {
        assert_eq!(parse_range("bytes=0-1", 1000), Ranged::Part(0, 1));
        assert_eq!(parse_range("bytes=990-", 1000), Ranged::Part(990, 999));
        assert_eq!(parse_range("bytes=990-5000", 1000), Ranged::Part(990, 999));
        // A suffix range is the last N bytes (players jump to an MP4's
        // index at the end of the file with it).
        assert_eq!(parse_range("bytes=-500", 1000), Ranged::Part(500, 999));
        assert_eq!(parse_range("bytes=-5000", 1000), Ranged::Part(0, 999));
        // Past the end, or nothing at all: 416.
        assert_eq!(parse_range("bytes=1000-", 1000), Ranged::Unsatisfiable);
        assert_eq!(parse_range("bytes=-0", 1000), Ranged::Unsatisfiable);
        // Not understood (other units, several ranges, junk): the whole file.
        assert_eq!(parse_range("items=0-1", 1000), Ranged::Whole);
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), Ranged::Whole);
        assert_eq!(parse_range("bytes=5-1", 1000), Ranged::Whole);
        assert_eq!(parse_range("bytes=x-1", 1000), Ranged::Whole);
    }

    fn film(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("rf-range-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("film.mp4");
        let bytes: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn with(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(k.clone(), v.parse().unwrap());
        }
        h
    }

    async fn body(r: Response) -> Vec<u8> {
        axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    #[tokio::test]
    async fn the_last_bytes_come_back_alone() {
        let path = film("suffix");
        let r = serve_file(&path, &with(&[(header::RANGE, "bytes=-10")]))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 990-999/1000");
        let want: Vec<u8> = (990..1000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(body(r).await, want);
    }

    #[tokio::test]
    async fn a_range_past_the_end_is_416() {
        let path = film("past");
        let r = serve_file(&path, &with(&[(header::RANGE, "bytes=5000-")]))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes */1000");
    }

    #[tokio::test]
    async fn the_file_carries_validators() {
        let path = film("validators");
        let r = serve_file(&path, &HeaderMap::new()).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let etag = r.headers()[header::ETAG].to_str().unwrap().to_string();
        assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
        let modified = r.headers()[header::LAST_MODIFIED].to_str().unwrap();
        assert!(modified.ends_with(" GMT"), "{modified}");

        // If-Range with the current tag: the range; with an old one: all of it.
        let r = serve_file(
            &path,
            &with(&[(header::RANGE, "bytes=0-1"), (header::IF_RANGE, &etag)]),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        let r = serve_file(
            &path,
            &with(&[(header::RANGE, "bytes=0-1"), (header::IF_RANGE, "\"old\"")]),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(body(r).await.len(), 1000);

        // A cached copy that is still current: 304, no body.
        let r = serve_file(&path, &with(&[(header::IF_NONE_MATCH, &etag)]))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::NOT_MODIFIED);
    }
}
