//! A link's sync: list the friend's films with their tags (cheap), fetch the
//! details of the new and changed ones, remove the ones gone, then merge.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use rustyfin_db::repo::links::{self, LinkedFilm};
use rustyfin_db::repo::media_streams::NewMediaStream;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{Link, SyncReport};

/// Details are fetched this many at a time.
const DETAIL_BATCH: usize = 100;

pub(super) async fn sync_link(pool: &PgPool, link: &Link) -> anyhow::Result<SyncReport> {
    let client = &link.client;
    let remote_libraries = client.libraries().await?;
    let mut wanted: Vec<(String, Uuid)> = Vec::new();
    for map in &link.settings.libraries {
        let Some((remote_id, _)) = remote_libraries
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(map.remote.trim()))
        else {
            let seen: Vec<&str> = remote_libraries.iter().map(|(_, n)| n.as_str()).collect();
            bail!(
                "{} shares no library named \"{}\" with its link user (it shares: {})",
                link.settings.name,
                map.remote,
                seen.join(", ")
            );
        };
        let local = rustyfin_db::repo::libraries::find_by_name(pool, map.local.trim())
            .await?
            .with_context(|| format!("there is no local library named \"{}\"", map.local))?;
        wanted.push((remote_id.clone(), local.id));
    }

    let stored = links::stored_copies(pool, link.id).await?;
    // The friend's film -> its stored copies' tags (one per version).
    let mut stored_tags: HashMap<&str, Vec<&Option<String>>> = HashMap::new();
    for (remote_id, (_, tag)) in &stored {
        stored_tags
            .entry(links::remote_item(remote_id))
            .or_default()
            .push(tag);
    }
    let mut report = SyncReport::default();
    // Stored remote ids still shared (every copy of an unchanged film, the
    // copies just written for a changed one).
    let mut kept: HashSet<String> = HashSet::new();
    for (remote_library, local_library) in wanted {
        let tags = client.film_tags(&remote_library).await?;
        let mut changed: Vec<String> = Vec::new();
        for (id, tag) in &tags {
            let current = tag.is_some()
                && stored_tags
                    .get(id.as_str())
                    .is_some_and(|stored| stored.iter().all(|t| *t == tag));
            if current {
                kept.extend(
                    stored
                        .keys()
                        .filter(|r| links::remote_item(r) == id)
                        .cloned(),
                );
            } else {
                changed.push(id.clone());
            }
        }
        for batch in changed.chunks(DETAIL_BATCH) {
            for item in client.items_by_id(batch).await? {
                for film in films_from_jellyfin(&item, link.id, &link.settings.name, local_library)
                {
                    kept.insert(film.remote_id.clone());
                    let (_, new) = links::upsert_film(pool, &film).await?;
                    if new {
                        report.added += 1;
                    } else {
                        report.updated += 1;
                    }
                }
            }
        }
    }
    let gone: Vec<String> = stored
        .keys()
        .filter(|id| !kept.contains(*id))
        .cloned()
        .collect();
    report.removed = links::delete_copies(pool, link.id, &gone).await?;
    report.merged = links::merge(pool).await?;
    Ok(report)
}

/// A Jellyfin film (`/Items` with ProviderIds, MediaSources...) as linked
/// copies in `library`: one per media source (Jellyfin's versions of the
/// film), the first stored under the film's id and the others as
/// `<film>/<source>`. Empty for anything that is not a film with an id.
pub fn films_from_jellyfin(
    item: &Value,
    link_id: Uuid,
    link_name: &str,
    library: Uuid,
) -> Vec<LinkedFilm> {
    let sources: Vec<Value> = item["MediaSources"]
        .as_array()
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![Value::Null]);
    sources
        .iter()
        .enumerate()
        .filter_map(|(i, source)| {
            film_from_source(item, source, i == 0, link_id, link_name, library)
        })
        .collect()
}

fn film_from_source(
    item: &Value,
    source: &Value,
    first: bool,
    link_id: Uuid,
    link_name: &str,
    library: Uuid,
) -> Option<LinkedFilm> {
    if item["Type"].as_str() != Some("Movie") {
        return None;
    }
    let item_id = item["Id"].as_str()?.to_string();
    let name = item["Name"].as_str()?.to_string();
    let source = source.clone();
    let streams: Vec<NewMediaStream> = source["MediaStreams"]
        .as_array()
        .map(|s| s.iter().filter_map(stream_from_jellyfin).collect())
        .unwrap_or_default();
    let video = streams.iter().find(|s| s.stream_type == "Video");
    let mut external_ids = serde_json::Map::new();
    if let Some(ids) = item["ProviderIds"].as_object() {
        for (k, v) in ids {
            if let Some(v) = v.as_str().filter(|v| !v.is_empty()) {
                external_ids.insert(k.clone(), Value::String(v.to_string()));
            }
        }
    }
    let mut images: Vec<(String, i32, String)> = Vec::new();
    if let Some(tags) = item["ImageTags"].as_object() {
        for (kind, tag) in tags {
            if let Some(tag) = tag.as_str() {
                images.push((kind.clone(), 0, tag.to_string()));
            }
        }
    }
    if let Some(tags) = item["BackdropImageTags"].as_array() {
        for (i, tag) in tags.iter().enumerate() {
            if let Some(tag) = tag.as_str() {
                images.push(("Backdrop".into(), i as i32, tag.to_string()));
            }
        }
    }
    let source_name = source["Name"].as_str().unwrap_or_default().to_string();
    let source_id = source["Id"].as_str().unwrap_or(&item_id).to_string();
    let remote_id = if first {
        item_id.clone()
    } else {
        format!("{item_id}/{source_id}")
    };
    Some(LinkedFilm {
        link_id,
        remote_etag: item["Etag"].as_str().map(str::to_string),
        library_id: library,
        sort_name: item["SortName"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| rustyfin_common::naming::make_sort_name(&name)),
        original_title: string(&item["OriginalTitle"]),
        overview: string(&item["Overview"]),
        community_rating: item["CommunityRating"].as_f64().map(|v| v as f32),
        critic_rating: item["CriticRating"].as_f64().map(|v| v as f32),
        official_rating: string(&item["OfficialRating"]),
        genres: strings(&item["Genres"]),
        studios: item["Studios"]
            .as_array()
            .map(|s| {
                s.iter()
                    .filter_map(|v| v["Name"].as_str().or(v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        tags: strings(&item["Tags"]),
        production_year: int(&item["ProductionYear"]),
        premiere_date: date(&item["PremiereDate"]),
        date_created: date(&item["DateCreated"]),
        runtime_ticks: item["RunTimeTicks"].as_i64(),
        container: string(&source["Container"]),
        total_bitrate: int(&source["Bitrate"]),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        size: source["Size"].as_i64(),
        external_ids: Value::Object(external_ids),
        extra_data: json!({
            "LinkName": link_name,
            "LinkSource": source_name,
            "RemoteSourceId": source_id,
            "RemoteItemId": item_id,
        }),
        streams,
        images,
        remote_id,
        name,
    })
}

fn stream_from_jellyfin(s: &Value) -> Option<NewMediaStream> {
    let stream_type = s["Type"].as_str()?.to_string();
    // External subtitles live on the friend's disk; not relayed yet.
    if s["IsExternal"].as_bool() == Some(true) {
        return None;
    }
    Some(NewMediaStream {
        item_id: Uuid::nil(),
        stream_index: int(&s["Index"]).unwrap_or(0),
        stream_type,
        codec: string(&s["Codec"]),
        language: string(&s["Language"]),
        title: string(&s["Title"]),
        is_default: s["IsDefault"].as_bool().unwrap_or(false),
        is_forced: s["IsForced"].as_bool().unwrap_or(false),
        is_external: false,
        width: int(&s["Width"]),
        height: int(&s["Height"]),
        bit_rate: int(&s["BitRate"]),
        bit_depth: int(&s["BitDepth"]),
        pixel_format: string(&s["PixelFormat"]),
        aspect_ratio: string(&s["AspectRatio"]),
        real_frame_rate: s["RealFrameRate"].as_f64().map(|v| v as f32),
        color_space: string(&s["ColorSpace"]),
        color_primaries: string(&s["ColorPrimaries"]),
        color_transfer: string(&s["ColorTransfer"]),
        channels: int(&s["Channels"]),
        channel_layout: string(&s["ChannelLayout"]),
        sample_rate: int(&s["SampleRate"]),
        profile: string(&s["Profile"]),
        level: s["Level"].as_f64().map(|v| v as i32),
        codec_tag: string(&s["CodecTag"]),
        time_base: string(&s["TimeBase"]),
        ref_frames: int(&s["RefFrames"]),
        nal_length_size: string(&s["NalLengthSize"]),
        is_interlaced: s["IsInterlaced"].as_bool().unwrap_or(false),
        path: None,
    })
}

fn string(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn int(v: &Value) -> Option<i32> {
    v.as_i64().and_then(|n| i32::try_from(n).ok())
}

fn date(v: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v.as_str()?)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "Id": "abc", "Name": "The Film", "Type": "Movie", "ProductionYear": 1999,
            "PremiereDate": "1999-03-31T00:00:00.0000000Z",
            "ProviderIds": {"Tmdb": "603", "Imdb": "tt0133093", "Tvdb": ""},
            "Studios": [{"Name": "Warner Bros.", "Id": "x"}],
            "Etag": "e9", "ImageTags": {"Primary": "pt"}, "BackdropImageTags": ["b0", "b1"],
            "MediaSources": [{"Id": "abc", "Name": "2160p", "Container": "mkv", "Size": 5,
                "Bitrate": 40_000_000, "MediaStreams": [
                    {"Index": 0, "Type": "Video", "Codec": "hevc", "Width": 3840, "Height": 1600,
                     "Level": 153, "ColorTransfer": "smpte2084"},
                    {"Index": 1, "Type": "Audio", "Codec": "truehd", "Channels": 8},
                    {"Index": 2, "Type": "Subtitle", "Codec": "srt", "IsExternal": true}
                ]}]
        })
    }

    #[test]
    fn a_jellyfin_film_becomes_a_linked_copy() {
        let link = Uuid::new_v4();
        let lib = Uuid::new_v4();
        let films = films_from_jellyfin(&sample(), link, "Alex", lib);
        assert_eq!(films.len(), 1);
        let f = films.into_iter().next().unwrap();
        assert_eq!(f.remote_id, "abc");
        assert_eq!(f.remote_etag.as_deref(), Some("e9"));
        assert_eq!(f.production_year, Some(1999));
        assert_eq!(f.external_ids, json!({"Tmdb": "603", "Imdb": "tt0133093"}));
        assert_eq!(f.studios, vec!["Warner Bros."]);
        assert_eq!((f.width, f.height), (Some(3840), Some(1600)));
        assert_eq!(f.total_bitrate, Some(40_000_000));
        assert_eq!(f.streams.len(), 2, "external subtitles are left out");
        assert_eq!(f.streams[0].level, Some(153));
        assert_eq!(f.images.len(), 3);
        assert_eq!(f.extra_data["LinkName"], "Alex");
        assert_eq!(f.extra_data["LinkSource"], "2160p");
        assert!(f.premiere_date.is_some());
        // No SortName from the friend: made as the scanner would.
        assert_eq!(
            f.sort_name,
            rustyfin_common::naming::make_sort_name("The Film")
        );
    }

    #[test]
    fn only_films_are_taken() {
        let mut v = sample();
        v["Type"] = json!("Series");
        assert!(films_from_jellyfin(&v, Uuid::nil(), "Alex", Uuid::nil()).is_empty());
    }

    #[test]
    fn each_version_is_a_copy_the_first_under_the_films_id() {
        let mut v = sample();
        let mut cut = v["MediaSources"][0].clone();
        cut["Id"] = json!("def");
        cut["Name"] = json!("1080p");
        cut["MediaStreams"][0]["Height"] = json!(800);
        v["MediaSources"].as_array_mut().unwrap().push(cut);
        let films = films_from_jellyfin(&v, Uuid::nil(), "Alex", Uuid::nil());
        let ids: Vec<&str> = films.iter().map(|f| f.remote_id.as_str()).collect();
        assert_eq!(ids, vec!["abc", "abc/def"]);
        assert_eq!(films[1].extra_data["RemoteSourceId"], "def");
        assert_eq!(films[1].extra_data["RemoteItemId"], "abc");
        assert_eq!(films[1].extra_data["LinkSource"], "1080p");
        assert_eq!(films[1].height, Some(800));
    }
}
