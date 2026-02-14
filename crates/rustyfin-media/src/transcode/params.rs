use std::path::PathBuf;

/// All parameters needed to build an ffmpeg transcode command.
#[derive(Debug, Clone)]
pub struct StreamParams {
    pub input_path: PathBuf,
    pub output_dir: PathBuf,
    pub start_time_ticks: Option<i64>,

    // Video
    pub video_codec: VideoCodec,
    pub video_bitrate: Option<i64>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_framerate: Option<f64>,

    // Audio
    pub audio_codec: AudioCodec,
    pub audio_bitrate: Option<i64>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub audio_stream_index: Option<i32>,

    // Subtitles
    pub subtitle_mode: SubtitleMode,
    pub subtitle_stream_index: Option<i32>,

    // HLS
    pub segment_duration: u32,
    pub segment_format: SegmentFormat,
    pub start_number: u32,

    // Hardware
    pub hardware_accel: HardwareAccel,
    pub threads: u32,

    // Audio-only mode (skips video stream mapping and encoding)
    pub audio_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    AAC,
    Opus,
    AC3,
    EAC3,
    FLAC,
    MP3,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleMode {
    None,
    Burn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentFormat {
    MpegTs,
    Fmp4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareAccel {
    None,
    Qsv,
    Vaapi,
    Nvenc,
}

impl HardwareAccel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "qsv" => Self::Qsv,
            "vaapi" => Self::Vaapi,
            "nvenc" => Self::Nvenc,
            _ => Self::None,
        }
    }
}

/// Convert Jellyfin 100ns ticks to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}
