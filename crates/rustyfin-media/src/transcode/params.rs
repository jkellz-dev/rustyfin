use std::path::PathBuf;

/// All parameters needed to build an ffmpeg transcode command.
#[derive(Debug, Clone)]
pub struct StreamParams {
    pub input_path: PathBuf,
    pub output_dir: PathBuf,
    pub start_time_ticks: Option<i64>,

    // Video
    pub video_codec: VideoCodec,
    /// The tag copied video is written with (`hvc1` for HEVC in fMP4).
    pub video_tag: Option<String>,
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

    /// Which ffmpeg run of the session this is: its files are
    /// `r<run>_<n>.<ext>`, so a restarted run never writes over a segment a
    /// player may be reading.
    pub run: u32,
    /// Where ffmpeg cuts segments.
    pub cut: SegmentCut,
    /// Encoded video: force keyframes at these file times, the later segment
    /// marks (and nowhere else), so the HLS muxer cuts exactly there. An
    /// expression on `t` does not do: with `-copyts` it counts from the run's
    /// start, so the marks moved with every restart.
    pub keyframes_at: Vec<f64>,
}

/// Where ffmpeg cuts a stream into segments.
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentCut {
    /// Every `segment_duration` seconds from the run's start (the HLS
    /// muxer): encoded video, whose only keyframes are on those marks, and
    /// audio.
    Every,
    /// At every keyframe (the HLS muxer): copied video in fMP4.
    EveryKeyframe,
    /// At these file times (the segment muxer): copied video in MPEG-TS,
    /// whose segments start on chosen keyframes.
    At(Vec<f64>),
}

impl StreamParams {
    /// The segment files' extension.
    pub fn segment_extension(&self) -> &'static str {
        match self.segment_format {
            SegmentFormat::MpegTs => "ts",
            SegmentFormat::Fmp4 => "m4s",
        }
    }

    /// The run's segment file name pattern, `%d` for the number.
    pub fn segment_pattern(&self) -> String {
        format!("r{}_%d.{}", self.run, self.segment_extension())
    }

    /// The run's list of finished segments: the HLS muxer's playlist or the
    /// segment muxer's CSV.
    pub fn list_file(&self) -> String {
        match self.cut {
            SegmentCut::At(_) => format!("r{}.csv", self.run),
            _ => format!("r{}.m3u8", self.run),
        }
    }

    /// The run's fMP4 init section.
    pub fn init_file(&self) -> String {
        format!("r{}_init.mp4", self.run)
    }
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

impl std::str::FromStr for HardwareAccel {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "qsv" => Self::Qsv,
            "vaapi" => Self::Vaapi,
            "nvenc" => Self::Nvenc,
            _ => Self::None,
        })
    }
}

/// Convert Jellyfin 100ns ticks to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}
