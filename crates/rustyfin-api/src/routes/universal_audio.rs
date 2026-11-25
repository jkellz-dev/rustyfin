use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use rustyfin_models::data::{Data, MediaItemStore};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;

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
/// HLS redirects to the audio playlist; otherwise the file is streamed from
/// this URL, as Jellyfin does (a redirect lost a token sent in the query).
pub async fn get_universal_audio(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<UniversalAudioParams>,
    Query(all_params): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // The id is written into the redirect URL below.
    let play_session_id = super::hls::session_id_param(params.play_session_id.as_deref())?;
    super::access::require_item_access(&state, auth.user_id, item_id).await?;

    // Verify item exists
    let _item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let protocol = params.transcoding_protocol.as_deref().unwrap_or("http");

    if protocol.eq_ignore_ascii_case("hls") {
        // Redirect to audio HLS master playlist
        let item_id_string = item_id.to_string();
        // Echoed into the Location header: encode, or a control character makes the
        // redirect panic.
        let media_source_id = rustyfin_media::transcode::names::encode_query_value(
            params.media_source_id.as_deref().unwrap_or(&item_id_string),
        );

        let mut url = format!(
            "/Audio/{item_id}/master.m3u8?PlaySessionId={play_session_id}&MediaSourceId={media_source_id}"
        );

        if let Some(ref ac) = params.audio_codec {
            url.push_str(&format!(
                "&AudioCodec={}",
                rustyfin_media::transcode::names::encode_query_value(ac)
            ));
        }
        if let Some(br) = params.max_streaming_bitrate {
            url.push_str(&format!("&MaxStreamingBitrate={br}"));
        }
        // The token the client authenticated with by query goes along.
        for (key, value) in &all_params {
            if key.eq_ignore_ascii_case("api_key") || key.eq_ignore_ascii_case("apikey") {
                url.push_str(&format!(
                    "&{key}={}",
                    rustyfin_media::transcode::names::encode_query_value(value)
                ));
            }
        }

        Ok(Redirect::temporary(&url).into_response())
    } else {
        super::streaming::serve_item_file(&state, item_id, &headers).await
    }
}
