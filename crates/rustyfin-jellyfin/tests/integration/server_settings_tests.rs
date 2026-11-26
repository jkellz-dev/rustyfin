//! Server settings in Jellyfin's shapes, read-only (owner decision
//! 2026-11-22: "your recommendations"). Jellyfin's dashboard and some apps
//! read these; Rustyfin's configuration is its TOML file, so the answers are
//! Jellyfin's defaults with what Rustyfin really does filled in.

use crate::common;

use axum::http::StatusCode;
use serde_json::Value;

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v
        .as_object()
        .unwrap_or_else(|| panic!("{v}"))
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

#[tokio::test]
async fn the_server_configuration_has_jellyfins_fields() {
    let app = common::TestApp::new().await;
    let (_, admin) = app.create_user("admin", true).await;

    let (status, c) = app.get("/System/Configuration", &admin).await;
    assert_eq!(status, StatusCode::OK);
    for key in [
        "MinResumePct",
        "MaxResumePct",
        "MinResumeDurationSeconds",
        "SortRemoveWords",
        "MetadataOptions",
        "TrickplayOptions",
        "CastReceiverApplications",
        "UICulture",
        "LogFileRetentionDays",
    ] {
        assert!(c.get(key).is_some(), "{key}: {c}");
    }
    // What Rustyfin really does, not Jellyfin's defaults.
    assert_eq!(c["ServerName"], "Rustyfin Test", "{c}");
    assert_eq!(c["QuickConnectAvailable"], false);
    assert_eq!(c["AllowClientLogUpload"], false);
    assert_eq!(c["PluginRepositories"], serde_json::json!([]));
    assert_eq!(c["IsStartupWizardCompleted"], true);
    // The play-state rules (play_state.rs) are Jellyfin's.
    assert_eq!(c["MinResumePct"], 5);
    assert_eq!(c["MaxResumePct"], 90);
    assert_eq!(c["MinResumeDurationSeconds"], 300);

    let (status, n) = app.get("/System/Configuration/network", &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert!(n["InternalHttpPort"].is_number(), "{n}");
    assert_eq!(n["EnableHttps"], false, "{n}");
    assert!(n["KnownProxies"].is_array(), "{n}");
    // Any casing of the key, as Jellyfin's lookup.
    let (_, upper) = app.get("/System/Configuration/Network", &admin).await;
    assert_eq!(keys(&upper), keys(&n));

    let (status, m) = app
        .get("/System/Configuration/MetadataOptions/Default", &admin)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(m["ImageFetcherOrder"].is_array(), "{m}");
    assert!(m.get("ItemType").is_none(), "{m}");

    let (status, s) = app.get("/Startup/Configuration", &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["UICulture"], "en-US", "{s}");
    assert!(s["MetadataCountryCode"].is_string(), "{s}");
    assert!(s["PreferredMetadataLanguage"].is_string(), "{s}");

    let (_, p) = app.get("/Auth/Providers", &admin).await;
    assert_eq!(p[0]["Id"], "Rustyfin.Auth.OidcProvider", "{p}");
    assert!(p[0]["Name"].is_string(), "{p}");

    // Settings are the administrator's.
    let (_, viewer) = app.create_user("viewer", false).await;
    let (status, _) = app
        .get("/System/Configuration/MetadataOptions/Default", &viewer)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn devices_and_keys_answer_in_jellyfins_shapes() {
    let app = common::TestApp::new().await;
    let (user, admin) = app.create_user("admin", true).await;
    rustyfin_data::repo::sessions::create(
        &app.pool,
        "tok-settings-tv",
        user,
        "tv-9",
        "Living Room TV",
        "Some Client",
        Some("2.0"),
    )
    .await
    .unwrap();

    let (_, list) = app.get("/Devices", &admin).await;
    let tv = list["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["Id"] == "tv-9")
        .cloned()
        .unwrap_or_else(|| panic!("{list}"));
    assert!(tv["DateLastActivity"].is_string(), "{tv}");
    assert!(tv["Capabilities"]["PlayableMediaTypes"].is_array(), "{tv}");
    assert!(
        tv["Capabilities"]["SupportsPersistentIdentifier"].is_boolean(),
        "{tv}"
    );

    let (status, one) = app.get("/Devices/Info?id=tv-9", &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["Name"], "Living Room TV", "{one}");
    assert_eq!(one["AppVersion"], "2.0", "{one}");
    let (status, _) = app.get("/Devices/Info?id=nowhere", &admin).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, opts) = app.get("/Devices/Options?id=tv-9", &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opts["DeviceId"], "tv-9", "{opts}");

    let (status, made) = app
        .post(
            "/Auth/Keys?App=Notifier",
            &serde_json::json!({"Name": "Notifier"}),
            &admin,
        )
        .await;
    assert!(status.is_success(), "{status} {made}");
    let (status, keys) = app.get("/Auth/Keys", &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys["TotalRecordCount"], 1, "{keys}");
    assert_eq!(keys["StartIndex"], 0, "{keys}");
    assert_eq!(keys["Items"][0]["AppName"], "Notifier", "{keys}");
    assert!(keys["Items"][0]["DateCreated"].is_string(), "{keys}");
    // Keys are shown once, when made; never listed.
    assert!(keys["Items"][0].get("AccessToken").is_none(), "{keys}");
    app.cleanup().await;
}
