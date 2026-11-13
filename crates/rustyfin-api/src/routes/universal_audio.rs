use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::media_items;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UniversalAudioParams {
    pub container: Option<String>,
    pub audio_codec: Option<String>,
    pub max_streaming_bitrate: Option<i64>,
    pub transcoding_protocol: Option<String>,
    pub play_session_id: Option<String>,
    pub media_source_id: Option<String>,
}

/// `GET /Audio/{itemId}/universal` — Universal audio endpoint.
///
/// Redirects to HLS transcoding or direct stream based on params.
pub async fn get_universal_audio(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<UniversalAudioParams>,
) -> Result<Response, ApiError> {
    // The id is written into the redirect URL below.
    let play_session_id = super::hls::session_id_param(params.play_session_id.as_deref())?;

    // Verify item exists
    let _item = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let protocol = params.transcoding_protocol.as_deref().unwrap_or("http");

    if protocol.eq_ignore_ascii_case("hls") {
        // Redirect to audio HLS master playlist
        let item_id_string = item_id.to_string();
        let media_source_id = params.media_source_id.as_deref().unwrap_or(&item_id_string);

        let mut url = format!(
            "/Audio/{item_id}/master.m3u8?PlaySessionId={play_session_id}&MediaSourceId={media_source_id}"
        );

        if let Some(ref ac) = params.audio_codec {
            url.push_str(&format!("&AudioCodec={ac}"));
        }
        if let Some(br) = params.max_streaming_bitrate {
            url.push_str(&format!("&MaxStreamingBitrate={br}"));
        }

        Ok(Redirect::temporary(&url).into_response())
    } else {
        // Default: redirect to direct audio stream
        let url = format!("/Audio/{item_id}/stream");
        Ok(Redirect::temporary(&url).into_response())
    }
}
