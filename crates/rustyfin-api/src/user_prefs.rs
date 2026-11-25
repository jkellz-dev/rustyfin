//! What a user's saved app settings (`POST /Users/{id}/Configuration`) do on
//! the server, as in Jellyfin (owner, 2026-11-19: "apply settings"):
//!
//! - `OrderedViews` orders their libraries; `MyMediaExcludes` hides libraries
//!   from them (`/UserViews?IncludeHidden=true` still lists them, for the
//!   app's own settings page).
//! - `LatestItemsExcludes` keeps libraries out of Latest across libraries;
//!   `HidePlayedInLatest` keeps played titles out of Latest.
//! - `AudioLanguagePreference`, `PlayDefaultAudioTrack`,
//!   `SubtitleLanguagePreference` and `SubtitleMode` choose each copy's
//!   `DefaultAudioStreamIndex` and `DefaultSubtitleStreamIndex` in
//!   PlaybackInfo, which apps start playback with.

use rustyfin_models::data::{Data, UserStore};
use rustyfin_models::dto::{BaseItemDto, MediaStream, PlaybackInfoResponse, UserConfiguration};
use uuid::Uuid;

/// The user's settings, or Jellyfin's defaults for a user who never saved any
/// (or cannot be read).
pub async fn load(data: &rustyfin_data::PgData, user_id: Uuid) -> UserConfiguration {
    match data.users().find_by_id(user_id).await {
        Ok(Some(row)) => crate::routes::users::user_configuration(&row),
        Ok(None) => UserConfiguration::default(),
        Err(e) => {
            tracing::warn!("users::find_by_id for settings: {e}");
            UserConfiguration::default()
        }
    }
}

/// Ids an app sent (with or without dashes).
pub fn ids(list: &[String]) -> Vec<Uuid> {
    list.iter()
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect()
}

/// The views in the user's order: those in `OrderedViews` first, in that
/// order, then the rest as they were; without `include_hidden`, those in
/// `MyMediaExcludes` left out.
pub fn arrange_views(
    views: &mut Vec<BaseItemDto>,
    config: &UserConfiguration,
    include_hidden: bool,
) {
    if !include_hidden {
        let hidden = ids(&config.my_media_excludes);
        views.retain(|v| !hidden.contains(&v.id));
    }
    let order = ids(&config.ordered_views);
    // Stable: unlisted views keep their order, after the listed ones.
    views.sort_by_key(|v| order.iter().position(|o| *o == v.id).unwrap_or(usize::MAX));
}

fn language_rank(stream: &MediaStream, preferred: &[String]) -> usize {
    stream
        .language
        .as_deref()
        .and_then(|l| preferred.iter().position(|p| p.eq_ignore_ascii_case(l)))
        .unwrap_or(usize::MAX)
}

fn preferences(list: Option<&str>) -> Vec<String> {
    list.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// The streams of one kind, those in a preferred language first (in the
/// order of preference), then embedded before external, then default first.
fn sorted<'a>(
    streams: &'a [MediaStream],
    kind: &str,
    preferred: &[String],
) -> Vec<&'a MediaStream> {
    let mut list: Vec<&MediaStream> = streams.iter().filter(|s| s.stream_type == kind).collect();
    list.sort_by_key(|s| (language_rank(s, preferred), s.is_external, !s.is_default));
    list
}

/// Jellyfin's default audio track: with `PlayDefaultAudioTrack`, the file's
/// default track whatever its language; otherwise (or with none marked) the
/// first in the preferred language.
pub fn default_audio(streams: &[MediaStream], config: &UserConfiguration) -> Option<i32> {
    let preferred = preferences(config.audio_language_preference.as_deref());
    let list = sorted(streams, "Audio", &preferred);
    if config.play_default_audio_track
        && let Some(s) = list.iter().find(|s| s.is_default)
    {
        return Some(s.index);
    }
    list.first().map(|s| s.index)
}

/// Jellyfin's default subtitle track for `SubtitleMode` (-1: none):
/// - `None`: none;
/// - `Default`: one marked default or forced, a preferred language first;
/// - `Smart`: when the audio is not in a preferred language, the first
///   subtitle in one; otherwise only a forced one (in a preferred language);
/// - `OnlyForced`: a forced one;
/// - `Always`: one in a preferred language, else a default or forced one,
///   else the first.
pub fn default_subtitle(
    streams: &[MediaStream],
    config: &UserConfiguration,
    audio: Option<i32>,
) -> i32 {
    let preferred = preferences(config.subtitle_language_preference.as_deref());
    let list = sorted(streams, "Subtitle", &preferred);
    let in_preferred = |s: &&&MediaStream| language_rank(s, &preferred) != usize::MAX;
    let marked = |s: &&&MediaStream| s.is_default || s.is_forced;
    let pick = match config.subtitle_mode.as_str() {
        "None" => None,
        "OnlyForced" => list.iter().find(|s| s.is_forced),
        "Always" => list
            .iter()
            .find(in_preferred)
            .or_else(|| list.iter().find(marked))
            .or_else(|| list.first()),
        "Smart" => {
            let audio_language = audio
                .and_then(|i| {
                    streams
                        .iter()
                        .find(|s| s.stream_type == "Audio" && s.index == i)
                })
                .and_then(|s| s.language.as_deref());
            let audio_preferred =
                audio_language.is_some_and(|l| preferred.iter().any(|p| p.eq_ignore_ascii_case(l)));
            if !preferred.is_empty() && !audio_preferred {
                list.iter().find(in_preferred)
            } else {
                // A forced one, in a preferred language when there are any.
                list.iter()
                    .find(|s| s.is_forced && (preferred.is_empty() || in_preferred(s)))
            }
        }
        // "Default", and anything an app may send that Rustyfin does not know.
        _ => list.iter().find(marked),
    };
    pick.map_or(-1, |s| s.index)
}

/// Each copy's default audio and subtitle tracks, for this user.
pub fn choose_tracks(info: &mut PlaybackInfoResponse, config: &UserConfiguration) {
    for source in &mut info.media_sources {
        let Some(streams) = source.media_streams.as_deref() else {
            continue;
        };
        let audio = default_audio(streams, config);
        let subtitle = default_subtitle(streams, config, audio);
        source.default_audio_stream_index = audio;
        source.default_subtitle_stream_index = Some(subtitle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(index: i32, kind: &str, lang: &str, default: bool, forced: bool) -> MediaStream {
        MediaStream {
            index,
            stream_type: kind.to_string(),
            language: Some(lang.to_string()),
            is_default: default,
            is_forced: forced,
            ..Default::default()
        }
    }

    fn film() -> Vec<MediaStream> {
        vec![
            stream(0, "Video", "und", true, false),
            stream(1, "Audio", "eng", true, false),
            stream(2, "Audio", "fre", false, false),
            stream(3, "Subtitle", "eng", false, false),
            stream(4, "Subtitle", "fre", false, false),
            stream(5, "Subtitle", "eng", false, true),
        ]
    }

    fn config(audio: &str, default_track: bool, subs: &str, mode: &str) -> UserConfiguration {
        UserConfiguration {
            audio_language_preference: Some(audio.to_string()),
            play_default_audio_track: default_track,
            subtitle_language_preference: Some(subs.to_string()),
            subtitle_mode: mode.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn audio_follows_the_language_unless_the_default_track_wins() {
        assert_eq!(
            default_audio(&film(), &config("fre", false, "", "Default")),
            Some(2)
        );
        assert_eq!(
            default_audio(&film(), &config("fre", true, "", "Default")),
            Some(1)
        );
        assert_eq!(
            default_audio(&film(), &config("", false, "", "Default")),
            Some(1)
        );
        assert_eq!(
            default_audio(&film(), &UserConfiguration::default()),
            Some(1)
        );
    }

    #[test]
    fn subtitles_follow_the_mode() {
        let s = film();
        assert_eq!(
            default_subtitle(&s, &config("", true, "fre", "None"), Some(1)),
            -1
        );
        assert_eq!(
            default_subtitle(&s, &config("", true, "fre", "Always"), Some(1)),
            4
        );
        assert_eq!(
            default_subtitle(&s, &config("", true, "", "OnlyForced"), Some(1)),
            5
        );
        assert_eq!(
            default_subtitle(&s, &config("", true, "", "Default"), Some(1)),
            5
        );
        // Smart: French subtitles for English audio; for French audio only a
        // forced one (none in French).
        assert_eq!(
            default_subtitle(&s, &config("", true, "fre", "Smart"), Some(1)),
            4
        );
        assert_eq!(
            default_subtitle(&s, &config("", true, "fre", "Smart"), Some(2)),
            -1
        );
        assert_eq!(
            default_subtitle(&s, &config("", true, "eng", "Smart"), Some(1)),
            5
        );
    }

    #[test]
    fn views_take_the_users_order_and_hidden_ones_go() {
        let [a, b, c] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let view = |id| BaseItemDto {
            id,
            ..Default::default()
        };
        let cfg = UserConfiguration {
            ordered_views: vec![c.simple().to_string(), a.to_string()],
            my_media_excludes: vec![b.simple().to_string()],
            ..Default::default()
        };
        let mut views = vec![view(a), view(b), view(c)];
        arrange_views(&mut views, &cfg, false);
        assert_eq!(views.iter().map(|v| v.id).collect::<Vec<_>>(), vec![c, a]);
        let mut views = vec![view(a), view(b), view(c)];
        arrange_views(&mut views, &cfg, true);
        assert_eq!(
            views.iter().map(|v| v.id).collect::<Vec<_>>(),
            vec![c, a, b]
        );
    }
}
