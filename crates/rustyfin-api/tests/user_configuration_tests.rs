//! `POST /Users/{userId}/Configuration` keeps what the app sends (Jellyfin's
//! web client saves its playback and display settings there), and every user
//! answer returns it. It was accepted and dropped.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn a_users_settings_are_kept_and_returned() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Movies", "movies").await;

    let config = json!({
        "AudioLanguagePreference": "jpn",
        "PlayDefaultAudioTrack": false,
        "SubtitleLanguagePreference": "eng",
        "SubtitleMode": "Always",
        "EnableNextEpisodeAutoPlay": false,
        "RememberAudioSelections": false,
        "OrderedViews": [lib.to_string()],
        "MyMediaExcludes": [lib.to_string()],
        "HidePlayedInLatest": false
    });
    let (status, _) = app
        .post(&format!("/Users/{user}/Configuration"), &config, &token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    for path in [format!("/Users/{user}"), "/Users/Me".to_string()] {
        let (status, dto) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        let c = &dto["Configuration"];
        assert_eq!(c["AudioLanguagePreference"], "jpn", "{path}: {c}");
        assert_eq!(c["PlayDefaultAudioTrack"], false, "{c}");
        assert_eq!(c["SubtitleLanguagePreference"], "eng", "{c}");
        assert_eq!(c["SubtitleMode"], "Always", "{c}");
        assert_eq!(c["EnableNextEpisodeAutoPlay"], false, "{c}");
        assert_eq!(c["RememberAudioSelections"], false, "{c}");
        assert_eq!(c["OrderedViews"], json!([lib.to_string()]), "{c}");
        assert_eq!(c["MyMediaExcludes"], json!([lib.to_string()]), "{c}");
        assert_eq!(c["HidePlayedInLatest"], false, "{c}");
        assert!(
            c["CastReceiverId"].as_str().is_some_and(|s| !s.is_empty()),
            "{c}"
        );
    }

    // Someone else's settings: only an administrator may change them.
    let (other, other_token) = app.create_user("other", false).await;
    let (status, _) = app
        .post(
            &format!("/Users/{user}/Configuration"),
            &config,
            &other_token,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, admin_token) = app.create_user("admin", true).await;
    let (status, _) = app
        .post(
            &format!("/Users/{other}/Configuration"),
            &json!({"SubtitleMode": "OnlyForced"}),
            &admin_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, dto) = app.get(&format!("/Users/{other}"), &admin_token).await;
    assert_eq!(dto["Configuration"]["SubtitleMode"], "OnlyForced");
    app.cleanup().await;
}
