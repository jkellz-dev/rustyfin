//! Validation for client-supplied names that become filesystem path components.
//!
//! A play session id names the transcode output directory, and a segment file name
//! is looked up inside it. Both arrive from the client (a query string and a URL
//! path segment that the router percent-decodes), so neither may contain a path
//! separator, a `..` component or anything else that makes `Path::join` leave the
//! directory it was given. The rules here are deliberately tighter than "no
//! separators": the real values are GUIDs, short tokens and the files ffmpeg writes.

/// Longest play session id accepted.
pub const MAX_SESSION_ID_LEN: usize = 128;

/// Longest segment or playlist file name accepted.
pub const MAX_SEGMENT_NAME_LEN: usize = 64;

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// True for a play session id that is safe to use as a directory name:
/// 1 to [`MAX_SESSION_ID_LEN`] bytes of ASCII letters, digits, `-` and `_`.
///
/// No dots at all, so `.` and `..` cannot occur, and no separators, control
/// characters or spaces.
pub fn is_valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_SESSION_ID_LEN && id.bytes().all(is_name_byte)
}

/// True for a segment or playlist file name that is safe to join onto a transcode
/// directory: `<stem>.<extension>` with exactly one dot, a stem of letters, digits,
/// `-` and `_`, and an alphanumeric extension. Covers `0.ts`, `12.m4s`, `init.mp4`
/// and `playlist.m3u8`, and nothing that begins with a dot.
pub fn is_valid_segment_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_SEGMENT_NAME_LEN {
        return false;
    }
    let Some((stem, ext)) = name.split_once('.') else {
        return false;
    };
    !stem.is_empty()
        && !ext.is_empty()
        && stem.bytes().all(is_name_byte)
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_that_clients_really_send_are_valid() {
        for id in [
            "unknown",
            "abc123",
            "0af3c1d2-7b1e-4c55-9d0a-5d2f6a1b9e11",
            "0af3c1d27b1e4c559d0a5d2f6a1b9e11",
            "play_1-x",
            "A",
        ] {
            assert!(is_valid_session_id(id), "{id}");
        }
        assert!(is_valid_session_id(&"a".repeat(MAX_SESSION_ID_LEN)));
    }

    #[test]
    fn session_ids_that_could_leave_the_directory_are_invalid() {
        for id in [
            "", ".", "..", "../x", "..\\x", "a/b", "a\\b", "/etc", "/", "C:\\x", "a b", "a\nb",
            "a\0b", "a.b", "ünï", "~", "a%2Fb",
        ] {
            assert!(!is_valid_session_id(id), "{id:?}");
        }
        assert!(!is_valid_session_id(&"a".repeat(MAX_SESSION_ID_LEN + 1)));
    }

    #[test]
    fn segment_names_ffmpeg_writes_are_valid() {
        for name in ["0.ts", "123.m4s", "init.mp4", "playlist.m3u8", "seg_01.ts"] {
            assert!(is_valid_segment_name(name), "{name}");
        }
    }

    #[test]
    fn segment_names_that_could_leave_the_directory_are_invalid() {
        for name in [
            "",
            ".",
            "..",
            "...",
            ".hidden",
            ".ts",
            "0.",
            "noextension",
            "a.b.ts",
            "../0.ts",
            "..\\0.ts",
            "/etc/passwd",
            "a/b.ts",
            "a\\b.ts",
            "0.ts\0.jpg",
            "0 .ts",
            "0.t s",
            "0.ts\n",
        ] {
            assert!(!is_valid_segment_name(name), "{name:?}");
        }
        let long = format!("{}.ts", "a".repeat(MAX_SEGMENT_NAME_LEN));
        assert!(!is_valid_segment_name(&long));
    }

    /// The point of the whole module: a valid name joined onto a base stays inside it.
    #[test]
    fn valid_names_join_inside_the_base_directory() {
        let base = std::path::Path::new("/tmp/transcodes");
        let session = base.join("abc-123");
        assert_eq!(session.parent(), Some(base));
        let segment = session.join("0.ts");
        assert_eq!(segment.parent(), Some(session.as_path()));
    }
}
