//! Reading and sending media files.
//!
//! [`probe`] (ffprobe) and [`audio_probe`] (lofty, in-process, for music)
//! read streams and tags; [`transcode`] runs HLS transcodes; [`streaming`]
//! serves files with HTTP ranges; [`image_processing`] resizes artwork.

pub mod audio_probe;
pub mod image_processing;
pub mod probe;
pub mod quality;
pub mod streaming;
pub mod transcode;
