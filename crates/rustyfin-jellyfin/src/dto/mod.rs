//! Jellyfin's request and response types (`BaseItemDto`, `MediaSourceInfo`,
//! `PlaybackInfoRequest`, session and user objects, ...), serialized in
//! Jellyfin's PascalCase.

pub mod activity;
pub mod auth;
pub mod base_item;
pub mod item_counts;
pub mod lyrics;
pub mod playback;
pub mod query_result;
pub mod remote_control;
pub mod session;
pub mod system;
pub mod theme_media;
pub mod user;
pub mod websocket;

pub use activity::*;
pub use auth::*;
pub use base_item::*;
pub use item_counts::*;
pub use playback::*;
pub use query_result::*;
pub use remote_control::*;
pub use session::*;
pub use system::*;
pub use theme_media::*;
pub use user::*;
pub use websocket::*;
