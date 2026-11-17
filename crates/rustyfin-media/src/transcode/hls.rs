use uuid::Uuid;

use super::params::{AudioCodec, VideoCodec};

/// Parameters for generating an HLS master playlist.
pub struct MasterPlaylistParams<'a> {
    pub item_id: Uuid,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub bitrate: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub play_session_id: &'a str,
    pub media_source_id: &'a str,
    pub query_string: &'a str,
    /// The video's RFC 6381 name when it is copied (the source's own profile
    /// and level); `None` uses the encoder's.
    pub video_codec_string: Option<&'a str>,
}

/// Generate an HLS master playlist pointing to our variant playlist endpoint.
pub fn generate_master_playlist(params: &MasterPlaylistParams<'_>) -> String {
    let mut playlist = String::from("#EXTM3U\n");

    // Build codec string
    let codecs = match params.video_codec_string {
        Some(v) => format!("{v},{}", audio_codec_string(params.audio_codec)),
        None => build_codec_string(params.video_codec, params.audio_codec),
    };

    // Build STREAM-INF line
    let bw = params.bitrate.unwrap_or(8_000_000);
    let mut stream_inf = format!("#EXT-X-STREAM-INF:BANDWIDTH={bw}");

    if let (Some(w), Some(h)) = (params.width, params.height) {
        stream_inf.push_str(&format!(",RESOLUTION={w}x{h}"));
    }

    stream_inf.push_str(&format!(",CODECS=\"{codecs}\""));
    playlist.push_str(&stream_inf);
    playlist.push('\n');

    // Variant playlist URL
    let item_id = params.item_id;
    let play_session_id = params.play_session_id;
    let media_source_id = params.media_source_id;
    let query_string = params.query_string;
    playlist.push_str(&format!(
        "/Videos/{item_id}/main.m3u8?PlaySessionId={play_session_id}&MediaSourceId={media_source_id}{query_string}\n"
    ));

    playlist
}

/// Generate an HLS master playlist for audio-only streams.
pub fn generate_audio_master_playlist(
    item_id: Uuid,
    audio_codec: AudioCodec,
    bitrate: Option<i64>,
    play_session_id: &str,
    media_source_id: &str,
    query_string: &str,
) -> String {
    let mut playlist = String::from("#EXTM3U\n");

    let codecs = audio_codec_string(audio_codec);
    let bw = bitrate.unwrap_or(192_000);

    playlist.push_str(&format!(
        "#EXT-X-STREAM-INF:BANDWIDTH={bw},CODECS=\"{codecs}\"\n"
    ));
    playlist.push_str(&format!(
        "/Audio/{item_id}/main.m3u8?PlaySessionId={play_session_id}&MediaSourceId={media_source_id}{query_string}\n"
    ));

    playlist
}

/// Build the RFC 6381 codec string for video + audio.
fn build_codec_string(video: VideoCodec, audio: AudioCodec) -> String {
    let v = video_codec_string(video);
    let a = audio_codec_string(audio);
    format!("{v},{a}")
}

/// RFC 6381 video codec string.
fn video_codec_string(codec: VideoCodec) -> &'static str {
    match codec {
        // H.264 High Profile, Level 4.1
        VideoCodec::H264 => "avc1.640029",
        // H.265 Main Profile, Level 4.0
        VideoCodec::H265 => "hvc1.1.6.L120.B0",
        // Copy preserves original codec — use H.264 as fallback since
        // we don't know the source codec at playlist generation time
        VideoCodec::Copy => "avc1.640029",
    }
}

/// RFC 6381 audio codec string.
fn audio_codec_string(codec: AudioCodec) -> &'static str {
    match codec {
        AudioCodec::AAC => "mp4a.40.2",
        AudioCodec::AC3 => "ac-3",
        AudioCodec::EAC3 => "ec-3",
        AudioCodec::Opus => "opus",
        AudioCodec::MP3 => "mp4a.40.34",
        AudioCodec::FLAC => "fLaC",
        AudioCodec::Copy => "mp4a.40.2",
    }
}

/// Rewrite ffmpeg's variant playlist to use our API segment URLs.
///
/// ffmpeg generates lines like `0.ts`, `1.ts` etc. We rewrite them to
/// `/Videos/{itemId}/hls1/{playSessionId}/0.ts`.
pub fn rewrite_variant_playlist(
    raw_playlist: &str,
    item_id: Uuid,
    play_session_id: &str,
) -> String {
    rewrite_playlist_segments(raw_playlist, "Videos", item_id, play_session_id, None)
}

/// Rewrite ffmpeg's variant playlist for audio streams.
///
/// Same as `rewrite_variant_playlist` but uses `/Audio/` prefix.
pub fn rewrite_audio_variant_playlist(
    raw_playlist: &str,
    item_id: Uuid,
    play_session_id: &str,
) -> String {
    rewrite_playlist_segments(raw_playlist, "Audio", item_id, play_session_id, None)
}

/// `rewrite_variant_playlist`, with the request's `api_key` on every segment.
pub fn rewrite_variant_playlist_with_key(
    raw_playlist: &str,
    item_id: Uuid,
    play_session_id: &str,
    api_key: Option<&str>,
) -> String {
    rewrite_playlist_segments(raw_playlist, "Videos", item_id, play_session_id, api_key)
}

/// `rewrite_audio_variant_playlist`, with the request's `api_key` on every segment.
pub fn rewrite_audio_variant_playlist_with_key(
    raw_playlist: &str,
    item_id: Uuid,
    play_session_id: &str,
    api_key: Option<&str>,
) -> String {
    rewrite_playlist_segments(raw_playlist, "Audio", item_id, play_session_id, api_key)
}

fn rewrite_playlist_segments(
    raw_playlist: &str,
    prefix: &str,
    item_id: Uuid,
    play_session_id: &str,
    api_key: Option<&str>,
) -> String {
    let mut result = String::with_capacity(raw_playlist.len() * 2);
    // Players that sign with `api_key` fetch segments without headers.
    let key = api_key
        .map(|k| format!("?api_key={}", super::names::encode_query_value(k)))
        .unwrap_or_default();

    for line in raw_playlist.lines() {
        if line.starts_with('#') || line.is_empty() {
            result.push_str(line);
        } else {
            result.push_str(&format!(
                "/{prefix}/{item_id}/hls1/{play_session_id}/{line}{key}"
            ));
        }
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_playlist_basic() {
        let playlist = generate_master_playlist(&MasterPlaylistParams {
            item_id: Uuid::nil(),
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::AAC,
            bitrate: Some(8_000_000),
            width: Some(1920),
            height: Some(1080),
            play_session_id: "session1",
            media_source_id: "source1",
            query_string: "",
            video_codec_string: None,
        });

        assert!(playlist.starts_with("#EXTM3U"));
        assert!(playlist.contains("BANDWIDTH=8000000"));
        assert!(playlist.contains("RESOLUTION=1920x1080"));
        assert!(playlist.contains("avc1.640029"));
        assert!(playlist.contains("mp4a.40.2"));
        assert!(playlist.contains("/Videos/"));
        assert!(playlist.contains("main.m3u8"));
        assert!(playlist.contains("PlaySessionId=session1"));
    }

    #[test]
    fn test_master_playlist_h265() {
        let playlist = generate_master_playlist(&MasterPlaylistParams {
            item_id: Uuid::nil(),
            video_codec: VideoCodec::H265,
            audio_codec: AudioCodec::AAC,
            bitrate: None,
            width: None,
            height: None,
            play_session_id: "s1",
            media_source_id: "m1",
            query_string: "",
            video_codec_string: None,
        });

        assert!(playlist.contains("hvc1.1.6.L120.B0"));
    }

    #[test]
    fn test_master_playlist_with_query_string() {
        let playlist = generate_master_playlist(&MasterPlaylistParams {
            item_id: Uuid::nil(),
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::AAC,
            bitrate: None,
            width: None,
            height: None,
            play_session_id: "s1",
            media_source_id: "m1",
            query_string: "&SubtitleStreamIndex=2",
            video_codec_string: None,
        });

        assert!(playlist.contains("SubtitleStreamIndex=2"));
    }

    #[test]
    fn test_audio_master_playlist() {
        let playlist = generate_audio_master_playlist(
            Uuid::nil(),
            AudioCodec::AAC,
            Some(192_000),
            "s1",
            "m1",
            "",
        );

        assert!(playlist.starts_with("#EXTM3U"));
        assert!(playlist.contains("BANDWIDTH=192000"));
        assert!(playlist.contains("/Audio/"));
    }

    #[test]
    fn test_rewrite_variant_playlist() {
        let raw = "#EXTM3U\n\
                    #EXT-X-VERSION:3\n\
                    #EXT-X-TARGETDURATION:6\n\
                    #EXTINF:6.000000,\n\
                    0.ts\n\
                    #EXTINF:6.000000,\n\
                    1.ts\n\
                    #EXTINF:4.500000,\n\
                    2.ts\n\
                    #EXT-X-ENDLIST\n";

        let item_id = Uuid::nil();
        let rewritten = rewrite_variant_playlist(raw, item_id, "session1");

        assert!(rewritten.contains(&format!("/Videos/{item_id}/hls1/session1/0.ts")));
        assert!(rewritten.contains(&format!("/Videos/{item_id}/hls1/session1/1.ts")));
        assert!(rewritten.contains(&format!("/Videos/{item_id}/hls1/session1/2.ts")));
        // Headers should be preserved
        assert!(rewritten.contains("#EXT-X-VERSION:3"));
        assert!(rewritten.contains("#EXTINF:6.000000,"));
    }

    #[test]
    fn test_codec_strings() {
        assert_eq!(video_codec_string(VideoCodec::H264), "avc1.640029");
        assert_eq!(video_codec_string(VideoCodec::H265), "hvc1.1.6.L120.B0");
        assert_eq!(audio_codec_string(AudioCodec::AAC), "mp4a.40.2");
        assert_eq!(audio_codec_string(AudioCodec::AC3), "ac-3");
        assert_eq!(audio_codec_string(AudioCodec::EAC3), "ec-3");
        assert_eq!(audio_codec_string(AudioCodec::Opus), "opus");
        assert_eq!(audio_codec_string(AudioCodec::FLAC), "fLaC");
    }

    #[test]
    fn segment_links_carry_the_key() {
        let raw = "#EXTM3U\n#EXTINF:6.0,\n0.ts\n#EXTINF:6.0,\n1.ts\n";
        let out = rewrite_variant_playlist_with_key(raw, Uuid::nil(), "ps", Some("tok"));
        let id = Uuid::nil();
        assert!(
            out.contains(&format!("/Videos/{id}/hls1/ps/0.ts?api_key=tok")),
            "{out}"
        );
        assert!(
            out.contains(&format!("/Videos/{id}/hls1/ps/1.ts?api_key=tok")),
            "{out}"
        );
        let plain = rewrite_variant_playlist_with_key(raw, Uuid::nil(), "ps", None);
        assert!(!plain.contains("api_key"), "{plain}");
        let audio = rewrite_audio_variant_playlist_with_key(raw, Uuid::nil(), "ps", Some("a b"));
        assert!(audio.contains("/Audio/"), "{audio}");
        assert!(
            audio.contains("0.ts?api_key=a%20b"),
            "the key is encoded: {audio}"
        );
    }
}
