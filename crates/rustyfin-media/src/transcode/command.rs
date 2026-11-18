use super::params::*;

/// Build the complete ffmpeg argument list from stream parameters.
pub fn build_ffmpeg_args(params: &StreamParams) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // 1. Global flags
    args.extend(build_global_args());

    // 2. Hardware acceleration input (before -i)
    args.extend(build_hw_accel_input_args(params));

    // 3. Seek (before input for fast seek)
    if let Some(ticks) = params.start_time_ticks
        && ticks > 0
    {
        let secs = ticks_to_seconds(ticks);
        args.extend(["-ss".to_string(), format!("{secs:.3}")]);
    }

    // 4. Input
    args.extend([
        "-i".to_string(),
        params.input_path.to_string_lossy().to_string(),
    ]);

    // 5. Stream mapping
    args.extend(build_stream_mapping(params));

    if params.audio_only {
        // Audio-only: disable video
        args.push("-vn".to_string());
    } else {
        // 6. Video encoding
        args.extend(build_video_args(params));

        // 8. Subtitle filter (burn-in)
        args.extend(build_subtitle_args(params));
    }

    // 7. Audio encoding
    args.extend(build_audio_args(params));

    // 9. Common output flags
    args.extend(build_common_output_args());

    // 10. Threading
    if params.threads > 0 {
        args.extend(["-threads".to_string(), params.threads.to_string()]);
    }

    // 11. HLS output
    args.extend(build_hls_args(params));

    args
}

fn build_global_args() -> Vec<String> {
    ["-hide_banner", "-nostdin", "-y"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn build_hw_accel_input_args(params: &StreamParams) -> Vec<String> {
    // Copied video is never decoded.
    if params.video_codec == VideoCodec::Copy {
        return vec![];
    }
    match params.hardware_accel {
        HardwareAccel::Qsv => vec![
            "-hwaccel".to_string(),
            "qsv".to_string(),
            "-hwaccel_output_format".to_string(),
            "qsv".to_string(),
        ],
        HardwareAccel::Vaapi => vec![
            "-hwaccel".to_string(),
            "vaapi".to_string(),
            "-hwaccel_output_format".to_string(),
            "vaapi".to_string(),
            "-vaapi_device".to_string(),
            "/dev/dri/renderD128".to_string(),
        ],
        HardwareAccel::Nvenc => vec![
            "-hwaccel".to_string(),
            "cuda".to_string(),
            "-hwaccel_output_format".to_string(),
            "cuda".to_string(),
        ],
        HardwareAccel::None => vec![],
    }
}

fn build_stream_mapping(params: &StreamParams) -> Vec<String> {
    let mut args = Vec::new();

    if !params.audio_only {
        args.extend(["-map".to_string(), "0:v:0".to_string()]);
    }

    let audio_index = params.audio_stream_index.unwrap_or(0);
    args.extend(["-map".to_string(), format!("0:a:{audio_index}")]);

    args
}

pub fn build_video_args(params: &StreamParams) -> Vec<String> {
    match params.video_codec {
        VideoCodec::Copy => {
            let mut args = vec!["-c:v".to_string(), "copy".to_string()];
            if let Some(tag) = &params.video_tag {
                args.extend(["-tag:v".to_string(), tag.clone()]);
            }
            args
        }
        VideoCodec::H264 => build_h264_args(params),
        VideoCodec::H265 => build_h265_args(params),
    }
}

fn build_h264_args(params: &StreamParams) -> Vec<String> {
    let mut args = match params.hardware_accel {
        HardwareAccel::Qsv => vec![
            "-c:v".to_string(),
            "h264_qsv".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-global_quality".to_string(),
            "23".to_string(),
        ],
        _ => vec![
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-crf".to_string(),
            "23".to_string(),
            "-profile:v".to_string(),
            "high".to_string(),
            "-level".to_string(),
            "4.1".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
        ],
    };

    // Bitrate cap
    if let Some(bitrate) = params.video_bitrate {
        args.extend([
            "-maxrate".to_string(),
            format!("{bitrate}"),
            "-bufsize".to_string(),
            format!("{}", bitrate * 2),
        ]);
    }

    if let Some(from) = params.keyframes_from {
        args.extend(forced_keyframes(params, from));
        let idr = match params.hardware_accel {
            HardwareAccel::Qsv => "-forced_idr",
            _ => "-forced-idr",
        };
        args.extend([idr.to_string(), "1".to_string()]);
        if params.hardware_accel == HardwareAccel::None {
            args.extend(["-sc_threshold".to_string(), "0".to_string()]);
        }
    } else {
        // Keyframe interval aligned to segment boundaries
        let fps = params.max_framerate.unwrap_or(24.0);
        let keyint = (params.segment_duration as f64 * fps) as u32;
        if params.hardware_accel == HardwareAccel::None {
            args.extend([
                "-g".to_string(),
                keyint.to_string(),
                "-sc_threshold".to_string(),
                "0".to_string(),
            ]);
        }
    }

    // Scale filter
    args.extend(build_scale_filter(params));

    args
}

fn build_h265_args(params: &StreamParams) -> Vec<String> {
    let mut args = match params.hardware_accel {
        HardwareAccel::Qsv => vec![
            "-c:v".to_string(),
            "hevc_qsv".to_string(),
            "-preset".to_string(),
            "fast".to_string(),
            "-global_quality".to_string(),
            "28".to_string(),
        ],
        _ => {
            // On the segment marks only when given; else a fixed interval.
            let keyint = if params.keyframes_from.is_some() {
                MARKS_ONLY_KEYINT
            } else {
                let fps = params.max_framerate.unwrap_or(24.0);
                (params.segment_duration as f64 * fps) as u32
            };
            vec![
                "-c:v".to_string(),
                "libx265".to_string(),
                "-preset".to_string(),
                "fast".to_string(),
                "-crf".to_string(),
                "28".to_string(),
                "-tag:v".to_string(),
                "hvc1".to_string(),
                "-x265-params".to_string(),
                format!("keyint={keyint}:min-keyint=1:scenecut=0"),
            ]
        }
    };
    if let Some(from) = params.keyframes_from {
        args.extend(forced_keyframes(params, from));
        let idr = match params.hardware_accel {
            HardwareAccel::Qsv => "-forced_idr",
            _ => "-forced-idr",
        };
        args.extend([idr.to_string(), "1".to_string()]);
    }

    // Bitrate cap
    if let Some(bitrate) = params.video_bitrate {
        args.extend([
            "-maxrate".to_string(),
            format!("{bitrate}"),
            "-bufsize".to_string(),
            format!("{}", bitrate * 2),
        ]);
    }

    // Scale filter
    args.extend(build_scale_filter(params));

    args
}

/// A keyframe interval long enough that the forced ones are the only ones.
const MARKS_ONLY_KEYINT: u32 = 100_000;

/// Keyframes on the segment marks only: from file time `from` (ffmpeg's
/// clock with `-copyts`) every `segment_duration` seconds. The HLS muxer
/// then cuts on exactly those marks, wherever the run started.
fn forced_keyframes(params: &StreamParams, from: f64) -> Vec<String> {
    vec![
        "-force_key_frames".to_string(),
        format!("expr:gte(t,{from:.6}+n_forced*{})", params.segment_duration),
        "-g".to_string(),
        MARKS_ONLY_KEYINT.to_string(),
    ]
}

fn build_scale_filter(params: &StreamParams) -> Vec<String> {
    match (params.max_width, params.max_height) {
        (Some(w), Some(h)) => vec![
            "-vf".to_string(),
            format!(
                "scale=min({w},iw):min({h},ih):force_original_aspect_ratio=decrease:force_divisible_by=2"
            ),
        ],
        (Some(w), None) => vec!["-vf".to_string(), format!("scale=min({w},iw):-2")],
        (None, Some(h)) => vec!["-vf".to_string(), format!("scale=-2:min({h},ih)")],
        (None, None) => vec![],
    }
}

pub fn build_audio_args(params: &StreamParams) -> Vec<String> {
    match params.audio_codec {
        AudioCodec::Copy => vec!["-c:a".to_string(), "copy".to_string()],
        AudioCodec::AAC => {
            let mut args = vec![
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                format!("{}k", params.audio_bitrate.unwrap_or(192_000) / 1000),
            ];
            if let Some(ch) = params.audio_channels {
                args.extend(["-ac".to_string(), ch.to_string()]);
            }
            if let Some(sr) = params.audio_sample_rate {
                args.extend(["-ar".to_string(), sr.to_string()]);
            } else {
                args.extend(["-ar".to_string(), "48000".to_string()]);
            }
            args
        }
        AudioCodec::AC3 => {
            let mut args = vec![
                "-c:a".to_string(),
                "ac3".to_string(),
                "-b:a".to_string(),
                format!("{}k", params.audio_bitrate.unwrap_or(640_000) / 1000),
            ];
            if let Some(ch) = params.audio_channels {
                args.extend(["-ac".to_string(), ch.to_string()]);
            }
            args
        }
        AudioCodec::EAC3 => {
            let mut args = vec![
                "-c:a".to_string(),
                "eac3".to_string(),
                "-b:a".to_string(),
                format!("{}k", params.audio_bitrate.unwrap_or(640_000) / 1000),
            ];
            if let Some(ch) = params.audio_channels {
                args.extend(["-ac".to_string(), ch.to_string()]);
            }
            args
        }
        AudioCodec::Opus => {
            let mut args = vec![
                "-c:a".to_string(),
                "libopus".to_string(),
                "-b:a".to_string(),
                format!("{}k", params.audio_bitrate.unwrap_or(128_000) / 1000),
                "-ar".to_string(),
                "48000".to_string(),
            ];
            if let Some(ch) = params.audio_channels {
                args.extend(["-ac".to_string(), ch.to_string()]);
            }
            args
        }
        AudioCodec::MP3 => {
            let mut args = vec![
                "-c:a".to_string(),
                "libmp3lame".to_string(),
                "-b:a".to_string(),
                format!("{}k", params.audio_bitrate.unwrap_or(192_000) / 1000),
            ];
            if let Some(sr) = params.audio_sample_rate {
                args.extend(["-ar".to_string(), sr.to_string()]);
            } else {
                args.extend(["-ar".to_string(), "44100".to_string()]);
            }
            args
        }
        AudioCodec::FLAC => vec!["-c:a".to_string(), "flac".to_string()],
    }
}

fn build_subtitle_args(params: &StreamParams) -> Vec<String> {
    match params.subtitle_mode {
        SubtitleMode::Burn => {
            if let Some(index) = params.subtitle_stream_index {
                // Text-based burn-in via subtitles filter
                let input = params.input_path.to_string_lossy();
                // Escape special characters for ffmpeg filter
                let escaped = input.replace('\\', "\\\\").replace(':', "\\:");
                vec![
                    "-vf".to_string(),
                    format!("subtitles='{escaped}':si={index}"),
                ]
            } else {
                vec![]
            }
        }
        SubtitleMode::None => vec![],
    }
}

fn build_common_output_args() -> Vec<String> {
    [
        "-map_metadata",
        "-1",
        "-map_chapters",
        "-1",
        "-max_muxing_queue_size",
        "2048",
        "-avoid_negative_ts",
        "disabled",
        "-copyts",
        // Timestamps as the file has them: the MPEG-TS muxer otherwise adds
        // 1.4 s, and the segments would sit after the playlist's times.
        "-muxdelay",
        "0",
        "-muxpreload",
        "0",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn build_hls_args(params: &StreamParams) -> Vec<String> {
    let dir = params.output_dir.to_string_lossy();
    let segments = format!("{dir}/{}", params.segment_pattern());
    let list = format!("{dir}/{}", params.list_file());

    if let SegmentCut::At(times) = &params.cut {
        // Copied MPEG-TS: the segment muxer cuts at the first keyframe at or
        // after each given time, so the cuts do not depend on where the run
        // started (the HLS muxer counts its cuts from there).
        let mut args = vec![
            "-f".to_string(),
            "segment".to_string(),
            "-segment_format".to_string(),
            "mpegts".to_string(),
        ];
        if times.is_empty() {
            // The film's last segment: nothing left to cut.
            args.extend(["-segment_time".to_string(), "1000000".to_string()]);
        } else {
            let times: Vec<String> = times.iter().map(f64::to_string).collect();
            args.extend(["-segment_times".to_string(), times.join(",")]);
        }
        args.extend([
            "-segment_start_number".to_string(),
            params.start_number.to_string(),
            "-segment_list".to_string(),
            list,
            "-segment_list_type".to_string(),
            "csv".to_string(),
            segments,
        ]);
        return args;
    }

    let hls_time = match params.cut {
        // Below any keyframe gap: every keyframe cuts.
        SegmentCut::EveryKeyframe => "0.01".to_string(),
        _ => params.segment_duration.to_string(),
    };
    let mut args = vec![
        "-f".to_string(),
        "hls".to_string(),
        "-hls_time".to_string(),
        hls_time,
    ];
    match params.segment_format {
        SegmentFormat::MpegTs => {
            args.extend(["-hls_segment_type".to_string(), "mpegts".to_string()]);
        }
        SegmentFormat::Fmp4 => {
            args.extend([
                "-hls_segment_type".to_string(),
                "fmp4".to_string(),
                "-hls_fmp4_init_filename".to_string(),
                params.init_file(),
            ]);
        }
    }
    args.extend([
        "-hls_segment_filename".to_string(),
        segments,
        "-hls_playlist_type".to_string(),
        "event".to_string(),
        "-hls_list_size".to_string(),
        "0".to_string(),
        "-hls_flags".to_string(),
        "temp_file".to_string(),
        "-start_number".to_string(),
        params.start_number.to_string(),
        // The run's own list of finished segments (must be last).
        list,
    ]);
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn default_params() -> StreamParams {
        StreamParams {
            input_path: PathBuf::from("/media/movie.mkv"),
            output_dir: PathBuf::from("/tmp/transcode/session1"),
            start_time_ticks: None,
            video_codec: VideoCodec::H264,
            video_tag: None,
            video_bitrate: Some(8_000_000),
            max_width: Some(1920),
            max_height: Some(1080),
            max_framerate: Some(24.0),
            audio_codec: AudioCodec::AAC,
            audio_bitrate: Some(192_000),
            audio_channels: Some(2),
            audio_sample_rate: Some(48000),
            audio_stream_index: None,
            subtitle_mode: SubtitleMode::None,
            subtitle_stream_index: None,
            segment_duration: 6,
            segment_format: SegmentFormat::MpegTs,
            start_number: 0,
            hardware_accel: HardwareAccel::None,
            threads: 0,
            audio_only: false,
            run: 0,
            cut: SegmentCut::Every,
            keyframes_from: None,
        }
    }

    #[test]
    fn test_basic_h264_aac_transcode() {
        let params = default_params();
        let args = build_ffmpeg_args(&params);

        // Global flags
        assert!(args.contains(&"-hide_banner".to_string()));
        assert!(args.contains(&"-nostdin".to_string()));
        assert!(args.contains(&"-y".to_string()));

        // Input
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/media/movie.mkv".to_string()));

        // Video codec
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));

        // Audio codec
        assert!(args.contains(&"-c:a".to_string()));
        assert!(args.contains(&"aac".to_string()));

        // HLS output
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"hls".to_string()));

        // Stream mapping
        assert!(args.contains(&"0:v:0".to_string()));
        assert!(args.contains(&"0:a:0".to_string()));
    }

    #[test]
    fn test_seek_position() {
        let mut params = default_params();
        params.start_time_ticks = Some(300_000_000); // 30 seconds

        let args = build_ffmpeg_args(&params);
        let ss_idx = args.iter().position(|a| a == "-ss").unwrap();
        assert_eq!(args[ss_idx + 1], "30.000");
    }

    #[test]
    fn test_no_seek_when_zero() {
        let mut params = default_params();
        params.start_time_ticks = Some(0);

        let args = build_ffmpeg_args(&params);
        assert!(!args.contains(&"-ss".to_string()));
    }

    #[test]
    fn test_copy_video_codec() {
        let mut params = default_params();
        params.video_codec = VideoCodec::Copy;

        let args = build_video_args(&params);
        assert_eq!(args, vec!["-c:v", "copy"]);
    }

    /// Copied HEVC is tagged `hvc1`, which Apple's players need (MKV files
    /// usually carry `hev1`).
    #[test]
    fn copied_video_carries_its_tag() {
        let mut params = default_params();
        params.video_codec = VideoCodec::Copy;
        params.video_tag = Some("hvc1".into());
        assert_eq!(
            build_video_args(&params),
            vec!["-c:v", "copy", "-tag:v", "hvc1"]
        );
    }

    /// Copied video is never decoded, so no hardware decoder is set up for
    /// it, and nothing touches the picture (no scaling, no keyframe forcing).
    #[test]
    fn copied_video_is_not_decoded() {
        let mut params = default_params();
        params.video_codec = VideoCodec::Copy;
        params.hardware_accel = HardwareAccel::Qsv;
        let args = build_ffmpeg_args(&params);
        assert!(!args.contains(&"-hwaccel".to_string()), "{args:?}");
        assert!(!args.contains(&"-vf".to_string()), "{args:?}");
        assert!(!args.contains(&"-g".to_string()), "{args:?}");
        let at = args.iter().position(|a| a == "-c:v").unwrap();
        assert_eq!(args[at + 1], "copy");
    }

    #[test]
    fn test_copy_audio_codec() {
        let mut params = default_params();
        params.audio_codec = AudioCodec::Copy;

        let args = build_audio_args(&params);
        assert_eq!(args, vec!["-c:a", "copy"]);
    }

    #[test]
    fn test_h265_software_encoding() {
        let mut params = default_params();
        params.video_codec = VideoCodec::H265;

        let args = build_video_args(&params);
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
    }

    #[test]
    fn test_qsv_h264_encoding() {
        let mut params = default_params();
        params.hardware_accel = HardwareAccel::Qsv;

        let args = build_ffmpeg_args(&params);

        // Should have QSV input flags
        assert!(args.contains(&"-hwaccel".to_string()));
        assert!(args.contains(&"qsv".to_string()));

        // Should use h264_qsv encoder
        assert!(args.contains(&"h264_qsv".to_string()));
    }

    #[test]
    fn test_audio_stream_selection() {
        let mut params = default_params();
        params.audio_stream_index = Some(2);

        let args = build_ffmpeg_args(&params);
        assert!(args.contains(&"0:a:2".to_string()));
    }

    #[test]
    fn test_fmp4_segment_format() {
        let mut params = default_params();
        params.segment_format = SegmentFormat::Fmp4;

        let args = build_hls_args(&params);
        assert!(args.contains(&"fmp4".to_string()));
        assert!(args.contains(&"-hls_fmp4_init_filename".to_string()));
        assert!(args.contains(&"r0_init.mp4".to_string()));
    }

    #[test]
    fn test_mpegts_segment_format() {
        let params = default_params();
        let args = build_hls_args(&params);
        assert!(args.contains(&"mpegts".to_string()));
        assert!(!args.contains(&"-hls_fmp4_init_filename".to_string()));
    }

    #[test]
    fn test_threading() {
        let mut params = default_params();
        params.threads = 4;

        let args = build_ffmpeg_args(&params);
        let threads_idx = args.iter().position(|a| a == "-threads").unwrap();
        assert_eq!(args[threads_idx + 1], "4");
    }

    #[test]
    fn test_no_threading_when_zero() {
        let params = default_params();
        let args = build_ffmpeg_args(&params);
        assert!(!args.contains(&"-threads".to_string()));
    }

    #[test]
    fn test_scale_filter_both_dimensions() {
        let params = default_params();
        let args = build_ffmpeg_args(&params);
        let vf_args: Vec<&String> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "-vf")
            .map(|(i, _)| &args[i + 1])
            .collect();
        assert!(
            vf_args
                .iter()
                .any(|a| a.contains("scale=min(1920,iw):min(1080,ih)"))
        );
    }

    #[test]
    fn test_no_scale_filter_when_no_dimensions() {
        let mut params = default_params();
        params.max_width = None;
        params.max_height = None;

        let args = build_video_args(&params);
        assert!(!args.iter().any(|a| a.contains("scale=")));
    }

    #[test]
    fn test_common_output_flags() {
        let params = default_params();
        let args = build_ffmpeg_args(&params);
        assert!(args.contains(&"-map_metadata".to_string()));
        assert!(args.contains(&"-1".to_string()));
        assert!(args.contains(&"-copyts".to_string()));
    }

    #[test]
    fn test_opus_audio() {
        let mut params = default_params();
        params.audio_codec = AudioCodec::Opus;
        params.audio_bitrate = None;

        let args = build_audio_args(&params);
        assert!(args.contains(&"libopus".to_string()));
        assert!(args.contains(&"128k".to_string()));
    }

    #[test]
    fn test_ac3_audio() {
        let mut params = default_params();
        params.audio_codec = AudioCodec::AC3;
        params.audio_bitrate = None;

        let args = build_audio_args(&params);
        assert!(args.contains(&"ac3".to_string()));
        assert!(args.contains(&"640k".to_string()));
    }

    #[test]
    fn test_mp3_audio() {
        let mut params = default_params();
        params.audio_codec = AudioCodec::MP3;
        params.audio_bitrate = None;
        params.audio_sample_rate = None;

        let args = build_audio_args(&params);
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(args.contains(&"44100".to_string()));
    }

    #[test]
    fn test_flac_audio() {
        let mut params = default_params();
        params.audio_codec = AudioCodec::FLAC;

        let args = build_audio_args(&params);
        assert_eq!(args, vec!["-c:a", "flac"]);
    }

    #[test]
    fn test_subtitle_burn_in() {
        let mut params = default_params();
        params.subtitle_mode = SubtitleMode::Burn;
        params.subtitle_stream_index = Some(0);

        let args = build_ffmpeg_args(&params);
        assert!(args.iter().any(|a| a.contains("subtitles=")));
    }

    #[test]
    fn test_h264_keyframe_interval() {
        let mut params = default_params();
        params.max_framerate = Some(30.0);
        params.segment_duration = 6;

        let args = build_video_args(&params);
        // 6 * 30 = 180
        let g_idx = args.iter().position(|a| a == "-g").unwrap();
        assert_eq!(args[g_idx + 1], "180");
    }

    #[test]
    fn test_bitrate_cap() {
        let mut params = default_params();
        params.video_bitrate = Some(4_000_000);

        let args = build_video_args(&params);
        let maxrate_idx = args.iter().position(|a| a == "-maxrate").unwrap();
        assert_eq!(args[maxrate_idx + 1], "4000000");
        let bufsize_idx = args.iter().position(|a| a == "-bufsize").unwrap();
        assert_eq!(args[bufsize_idx + 1], "8000000");
    }

    #[test]
    fn test_start_number() {
        let mut params = default_params();
        params.start_number = 42;

        let args = build_hls_args(&params);
        let sn_idx = args.iter().position(|a| a == "-start_number").unwrap();
        assert_eq!(args[sn_idx + 1], "42");
    }

    #[test]
    fn test_playlist_output_path() {
        let params = default_params();
        let args = build_hls_args(&params);
        assert!(args.last().unwrap().ends_with("/r0.m3u8"));
    }

    /// Encodes put keyframes on the segment marks only, counted in file time
    /// from the run's first segment, so a restarted run cuts where the
    /// playlist says.
    #[test]
    fn encodes_put_keyframes_on_the_segment_marks() {
        let mut params = default_params();
        params.keyframes_from = Some(2466.0);
        let args = build_video_args(&params);
        let at = args.iter().position(|a| a == "-force_key_frames").unwrap();
        assert_eq!(args[at + 1], "expr:gte(t,2466.000000+n_forced*6)");
        let g = args.iter().position(|a| a == "-g").unwrap();
        assert_eq!(args[g + 1], "100000", "no other keyframes");
        assert!(
            args.windows(2).any(|w| w == ["-forced-idr", "1"]),
            "{args:?}"
        );
        params.video_codec = VideoCodec::H265;
        let args = build_video_args(&params);
        assert!(
            args.contains(&"expr:gte(t,2466.000000+n_forced*6)".to_string()),
            "{args:?}"
        );
        assert!(args.iter().any(|a| a.contains("scenecut=0")), "{args:?}");
    }

    #[test]
    fn segment_files_carry_the_run_number() {
        let mut params = default_params();
        params.run = 3;
        params.start_number = 20;
        let args = build_hls_args(&params);
        let at = args
            .iter()
            .position(|a| a == "-hls_segment_filename")
            .unwrap();
        assert_eq!(args[at + 1], "/tmp/transcode/session1/r3_%d.ts");
        assert!(args.last().unwrap().ends_with("/r3.m3u8"));
        assert!(args.windows(2).any(|w| w == ["-hls_time", "6"]), "{args:?}");
        assert!(
            args.windows(2).any(|w| w == ["-start_number", "20"]),
            "{args:?}"
        );
    }

    /// Copied video in MPEG-TS goes through the segment muxer, which cuts at
    /// the given keyframe times wherever the run started.
    #[test]
    fn copied_ts_is_cut_at_the_given_keyframes() {
        let mut params = default_params();
        params.video_codec = VideoCodec::Copy;
        params.run = 2;
        params.start_number = 3;
        params.cut = SegmentCut::At(vec![30.9995, 37.9995]);
        let args = build_hls_args(&params);
        assert!(args.windows(2).any(|w| w == ["-f", "segment"]), "{args:?}");
        assert!(
            args.windows(2)
                .any(|w| w == ["-segment_times", "30.9995,37.9995"]),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|w| w == ["-segment_start_number", "3"]),
            "{args:?}"
        );
        let list = args.iter().position(|a| a == "-segment_list").unwrap();
        assert_eq!(args[list + 1], "/tmp/transcode/session1/r2.csv");
        assert_eq!(args.last().unwrap(), "/tmp/transcode/session1/r2_%d.ts");
        // The last segment: nothing left to cut.
        params.cut = SegmentCut::At(vec![]);
        let args = build_hls_args(&params);
        assert!(!args.contains(&"-segment_times".to_string()), "{args:?}");
    }

    /// Copied video in fMP4: the HLS muxer cuts at every keyframe.
    #[test]
    fn copied_fmp4_is_cut_at_every_keyframe() {
        let mut params = default_params();
        params.video_codec = VideoCodec::Copy;
        params.segment_format = SegmentFormat::Fmp4;
        params.cut = SegmentCut::EveryKeyframe;
        let args = build_hls_args(&params);
        assert!(
            args.windows(2).any(|w| w == ["-hls_time", "0.01"]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["-hls_fmp4_init_filename", "r0_init.mp4"]),
            "{args:?}"
        );
    }

    /// The MPEG-TS muxer's default 1.4 s delay would put every segment's
    /// times 1.4 s after the playlist's.
    #[test]
    fn timestamps_are_written_as_the_file_has_them() {
        let args = build_ffmpeg_args(&default_params());
        assert!(args.windows(2).any(|w| w == ["-muxdelay", "0"]), "{args:?}");
        assert!(
            args.windows(2).any(|w| w == ["-muxpreload", "0"]),
            "{args:?}"
        );
    }
}
