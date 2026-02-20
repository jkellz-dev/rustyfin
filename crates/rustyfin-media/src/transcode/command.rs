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
        VideoCodec::Copy => vec!["-c:v".to_string(), "copy".to_string()],
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
            let fps = params.max_framerate.unwrap_or(24.0);
            let keyint = (params.segment_duration as f64 * fps) as u32;
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
                format!("keyint={keyint}:min-keyint={keyint}:scenecut=0"),
            ]
        }
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

    // Scale filter
    args.extend(build_scale_filter(params));

    args
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
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn build_hls_args(params: &StreamParams) -> Vec<String> {
    let mut args = vec!["-f".to_string(), "hls".to_string()];

    args.extend(["-hls_time".to_string(), params.segment_duration.to_string()]);

    match params.segment_format {
        SegmentFormat::MpegTs => {
            args.extend(["-hls_segment_type".to_string(), "mpegts".to_string()]);
            args.extend([
                "-hls_segment_filename".to_string(),
                format!("{}/%d.ts", params.output_dir.to_string_lossy()),
            ]);
        }
        SegmentFormat::Fmp4 => {
            args.extend(["-hls_segment_type".to_string(), "fmp4".to_string()]);
            args.extend([
                "-hls_fmp4_init_filename".to_string(),
                "init.mp4".to_string(),
            ]);
            args.extend([
                "-hls_segment_filename".to_string(),
                format!("{}/%d.m4s", params.output_dir.to_string_lossy()),
            ]);
        }
    }

    args.extend([
        "-hls_playlist_type".to_string(),
        "event".to_string(),
        "-hls_list_size".to_string(),
        "0".to_string(),
        "-hls_flags".to_string(),
        "temp_file".to_string(),
        "-start_number".to_string(),
        params.start_number.to_string(),
    ]);

    // Output playlist path (must be last argument)
    args.push(format!(
        "{}/playlist.m3u8",
        params.output_dir.to_string_lossy()
    ));

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
        assert!(args.contains(&"init.mp4".to_string()));
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
        assert!(args.last().unwrap().ends_with("playlist.m3u8"));
    }
}
