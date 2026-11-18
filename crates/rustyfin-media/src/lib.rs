//! Reading and sending media files.
//!
//! [`probe`] (ffprobe) and [`audio_probe`] (lofty, in-process, for music)
//! read streams and tags; [`playback_info`] answers `PlaybackInfo` and, with
//! [`transcode`], decides direct play, direct stream or an HLS transcode
//! from the client's device profile; [`streaming`] serves files with HTTP
//! ranges; [`image_processing`] resizes artwork.

pub mod audio_probe;
pub mod image_processing;
pub mod playback_info;
pub mod probe;
pub mod quality;
pub mod streaming;
pub mod transcode;
