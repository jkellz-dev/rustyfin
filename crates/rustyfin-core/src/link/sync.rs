//! A link's sync: list the friend's films with their tags (cheap), fetch the
//! details of the new and changed ones, remove the ones gone, then merge. A
//! Rustyfin friend says what changed itself (its change feed).

use rustyfin_models::data::{Data, LibraryStore};
use std::collections::{HashMap, HashSet};

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use rustyfin_data::PgData;
use rustyfin_data::repo::links::{self, LinkedItem};
use rustyfin_data::repo::media_streams::NewMediaStream;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Link, SyncReport};

/// Details are fetched this many at a time.
const DETAIL_BATCH: usize = 100;

pub(super) async fn sync_link(data: &PgData, link: &Link) -> anyhow::Result<SyncReport> {
    if link.settings.kind == super::LinkKind::Rustyfin {
        return sync_rustyfin(data, link).await;
    }
    let client = &link.client;
    // Signing in and listing the friend's libraries must work, or nothing can.
    let remote_libraries = client.libraries().await?;
    let stored = links::stored_copies(data, link.id).await?;
    // The friend's item -> its stored copies' tags (one per version).
    let mut stored_tags: HashMap<&str, Vec<&Option<String>>> = HashMap::new();
    for (remote_id, (_, tag)) in &stored {
        stored_tags
            .entry(links::remote_item(remote_id))
            .or_default()
            .push(tag);
    }
    let stored = Stored {
        copies: &stored,
        tags: stored_tags,
    };
    let mut progress = Progress::default();
    for map in &link.settings.libraries {
        let mapped = sync_library(data, link, map, &remote_libraries, &stored, &mut progress).await;
        if let Err(e) = mapped {
            progress
                .report
                .errors
                .push(format!("{}: {e:#}", map.remote));
        }
    }
    let Progress { mut report, kept } = progress;
    let stored = stored.copies;
    // A mapping that failed may only be out of sight for now: remove nothing.
    if report.errors.is_empty() {
        let gone: Vec<String> = stored
            .keys()
            .filter(|id| !kept.contains(*id))
            .cloned()
            .collect();
        report.removed = links::delete_copies(data, link.id, &gone).await?;
    }
    report.merged = links::merge(data).await?;
    sync_collections(data, link, &mut report).await?;
    Ok(report)
}

/// The friend's collections: store them, and when they or the link's films
/// changed, make the collections of the films libraries the link fills again.
async fn sync_collections(
    data: &PgData,
    link: &Link,
    report: &mut SyncReport,
) -> anyhow::Result<()> {
    let sets = match link.client.collections().await {
        Ok(sets) => sets,
        Err(e) => {
            report.errors.push(format!("collections: {e:#}"));
            return Ok(());
        }
    };
    let changed = links::set_collections(data, link.id, &sets).await?;
    let films_changed = report.added + report.updated + report.removed + report.merged > 0;
    if changed || films_changed {
        for library in links::movie_libraries(data, link.id).await? {
            crate::library::scanner::rebuild_movie_collections(data, library).await?;
        }
    }
    Ok(())
}

/// What the link held before this sync.
struct Stored<'a> {
    /// Remote id -> (row id, the friend's tag).
    copies: &'a HashMap<String, (Uuid, Option<String>)>,
    /// The friend's item -> its stored copies' tags (one per version).
    tags: HashMap<&'a str, Vec<&'a Option<String>>>,
}

/// What this sync has done so far.
#[derive(Default)]
struct Progress {
    report: SyncReport,
    /// Stored remote ids still shared (every copy of an unchanged item, the
    /// copies just written for a changed one).
    kept: HashSet<String>,
}

/// One `[[link.libraries]]`: find (or make) the local library, list the
/// friend's items with their tags, store the new and changed ones.
async fn sync_library(
    data: &PgData,
    link: &Link,
    map: &super::LibraryMap,
    remote_libraries: &[(String, String, String)],
    stored: &Stored<'_>,
    progress: &mut Progress,
) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let client = &link.client;
    let (remote_id, local_library, shows) = target(data, link, map, remote_libraries).await?;
    let types = if shows {
        "Series,Season,Episode"
    } else {
        "Movie,Video"
    };
    let tags = client.item_tags(&remote_id, types).await?;
    let mut changed: Vec<String> = Vec::new();
    for (id, tag) in &tags {
        let current = tag.is_some()
            && stored
                .tags
                .get(id.as_str())
                .is_some_and(|stored| stored.iter().all(|t| *t == tag));
        if current {
            progress.kept.extend(
                stored
                    .copies
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
            for film in items_from_jellyfin(&item, link.id, &link.settings.name, local_library) {
                progress.kept.insert(film.remote_id.clone());
                let (_, new) = links::upsert_item(data, &film).await?;
                if new {
                    progress.report.added += 1;
                } else {
                    progress.report.updated += 1;
                }
            }
        }
    }
    // A library of its own has no folders to scan: this sync is what fills it.
    if map.local.is_none() {
        data.libraries()
            .mark_scanned(local_library, started.elapsed())
            .await?;
    }
    Ok(())
}

/// Where a `[[link.libraries]]` goes: the friend's library id, the local
/// library (made for the link when there is no `local`), and whether it holds
/// shows.
async fn target(
    data: &PgData,
    link: &Link,
    map: &super::LibraryMap,
    remote_libraries: &[(String, String, String)],
) -> anyhow::Result<(String, Uuid, bool)> {
    let Some((remote_id, remote_name, collection)) = remote_libraries
        .iter()
        .find(|(_, name, _)| name.eq_ignore_ascii_case(map.remote.trim()))
    else {
        let seen: Vec<&str> = remote_libraries
            .iter()
            .map(|(_, n, _)| n.as_str())
            .collect();
        bail!(
            "{} shares no library named \"{}\" with its link user (it shares: {})",
            link.settings.name,
            map.remote,
            seen.join(", ")
        );
    };
    let shows = collection == "tvshows";
    let local_library = match &map.local {
        Some(local) => {
            data.libraries()
                .find_by_name(local.trim())
                .await?
                .with_context(|| format!("there is no local library named \"{local}\""))?
                .id
        }
        None => {
            links::ensure_library(
                data,
                link.id,
                &format!("{remote_name} · {}", link.settings.name),
                if shows { "tvshows" } else { "movies" },
            )
            .await?
        }
    };
    Ok((remote_id.clone(), local_library, shows))
}

/// An id as one form: no dashes, lower case (a Rustyfin writes its ids both
/// ways).
fn plain(id: &str) -> String {
    id.replace('-', "").to_ascii_lowercase()
}

/// A Rustyfin friend (roadmap 16b.2r): what changed since the last sync comes
/// from its change feed (`/api/v1/catalog`), so a quiet sync is one call and a
/// busy one fetches only what changed. The first sync (or one after the
/// friend refused the cursor) takes everything and removes what it lacks.
async fn sync_rustyfin(data: &PgData, link: &Link) -> anyhow::Result<SyncReport> {
    let started = std::time::Instant::now();
    let client = &link.client;
    let remote_libraries = client.libraries().await?;
    let mut report = SyncReport::default();
    // The friend's library -> (local library, whether it holds shows).
    let mut targets: HashMap<String, (Uuid, bool)> = HashMap::new();
    // Libraries of their own, which only this sync fills.
    let mut own: Vec<Uuid> = Vec::new();
    for map in &link.settings.libraries {
        match target(data, link, map, &remote_libraries).await {
            Ok((remote, local, shows)) => {
                targets.insert(plain(&remote), (local, shows));
                if map.local.is_none() {
                    own.push(local);
                }
            }
            Err(e) => report.errors.push(format!("{}: {e:#}", map.remote)),
        }
    }
    let mut since = links::catalog_cursor(data, link.id).await?;
    let feed = match client.catalog(since.as_deref()).await? {
        Some(feed) => feed,
        None => {
            since = None;
            client
                .catalog(None)
                .await?
                .with_context(|| format!("{} refused its own feed", link.settings.name))?
        }
    };
    let full = since.is_none();
    let stored = links::stored_copies(data, link.id).await?;
    // What to fetch, per local library.
    let mut wanted: HashMap<Uuid, Vec<String>> = HashMap::new();
    for changed in &feed.changed {
        let Some((local, shows)) = targets.get(&plain(&changed.library)) else {
            continue;
        };
        let fits = if *shows {
            matches!(changed.item_type.as_str(), "Series" | "Season" | "Episode")
        } else {
            matches!(changed.item_type.as_str(), "Movie" | "Video")
        };
        if fits {
            wanted.entry(*local).or_default().push(changed.id.clone());
        }
    }
    let mut kept: HashSet<String> = HashSet::new();
    for (local, ids) in &wanted {
        for batch in ids.chunks(DETAIL_BATCH) {
            for item in client.items_by_id(batch).await? {
                let Some(item_id) = item["Id"].as_str().map(plain) else {
                    continue;
                };
                let copies = items_from_jellyfin(&item, link.id, &link.settings.name, *local);
                let written: HashSet<String> = copies.iter().map(|c| c.remote_id.clone()).collect();
                for copy in copies {
                    let (_, new) = links::upsert_item(data, &copy).await?;
                    if new {
                        report.added += 1;
                    } else {
                        report.updated += 1;
                    }
                }
                // A version the friend no longer keeps of an item it does.
                let stale: Vec<String> = stored
                    .keys()
                    .filter(|r| plain(links::remote_item(r)) == item_id && !written.contains(*r))
                    .cloned()
                    .collect();
                report.removed += links::delete_copies(data, link.id, &stale).await?;
                kept.extend(written);
            }
        }
    }
    let gone: Vec<String> = if full {
        // A mapping that failed may only be out of sight for now: remove nothing.
        if report.errors.is_empty() {
            stored
                .keys()
                .filter(|r| !kept.contains(*r))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    } else {
        let removed: HashSet<String> = feed.removed.iter().map(|r| plain(r)).collect();
        stored
            .keys()
            .filter(|r| removed.contains(&plain(links::remote_item(r))))
            .cloned()
            .collect()
    };
    report.removed += links::delete_copies(data, link.id, &gone).await?;
    // With a mapping failing, its changes come again next time.
    if report.errors.is_empty() {
        links::set_catalog_cursor(data, link.id, &feed.cursor).await?;
        for library in own {
            data.libraries()
                .mark_scanned(library, started.elapsed())
                .await?;
        }
    }
    report.merged = links::merge(data).await?;
    Ok(report)
}

/// A Jellyfin item (`/Items` with ProviderIds, MediaSources...) as linked
/// copies in `library`. A film (or home video, stored as a film) or an
/// episode gives one per media source (Jellyfin's versions of it), the first
/// stored under the item's id and the others as `<item>/<source>`; a series or
/// season gives one. Empty for any other kind, or an item with no id.
pub fn items_from_jellyfin(
    item: &Value,
    link_id: Uuid,
    link_name: &str,
    library: Uuid,
) -> Vec<LinkedItem> {
    let sources: Vec<Value> = match item["Type"].as_str() {
        Some("Series" | "Season") => vec![Value::Null],
        _ => item["MediaSources"]
            .as_array()
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![Value::Null]),
    };
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
) -> Option<LinkedItem> {
    let item_type = match item["Type"].as_str()? {
        "Movie" | "Video" => "Movie",
        "Series" => "Series",
        "Season" => "Season",
        "Episode" => "Episode",
        _ => return None,
    };
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
    Some(LinkedItem {
        item_type: item_type.to_string(),
        index_number: int(&item["IndexNumber"]),
        parent_index_number: int(&item["ParentIndexNumber"]),
        is_folder: matches!(item_type, "Series" | "Season"),
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
            "RemoteSeriesId": item["SeriesId"].as_str(),
            "RemoteSeasonId": item["SeasonId"].as_str().or(item["ParentId"].as_str().filter(|_| item_type == "Episode")),
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
        let films = items_from_jellyfin(&sample(), link, "Alex", lib);
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
    fn only_films_videos_and_shows_are_taken() {
        let mut v = sample();
        v["Type"] = json!("BoxSet");
        assert!(items_from_jellyfin(&v, Uuid::nil(), "Alex", Uuid::nil()).is_empty());
    }

    #[test]
    fn home_videos_are_films_and_shows_keep_their_place() {
        let mut v = sample();
        v["Type"] = json!("Video");
        let films = items_from_jellyfin(&v, Uuid::nil(), "Alex", Uuid::nil());
        assert_eq!(films[0].item_type, "Movie");

        let season = json!({"Id": "s1", "Name": "Season 2", "Type": "Season", "IndexNumber": 2,
                            "SeriesId": "show", "ParentId": "show", "Etag": "e"});
        let rows = items_from_jellyfin(&season, Uuid::nil(), "Alex", Uuid::nil());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_type, "Season");
        assert!(rows[0].is_folder);
        assert_eq!(rows[0].index_number, Some(2));
        assert_eq!(rows[0].extra_data["RemoteSeriesId"], "show");

        let mut ep = sample();
        ep["Type"] = json!("Episode");
        ep["IndexNumber"] = json!(3);
        ep["ParentIndexNumber"] = json!(2);
        ep["SeriesId"] = json!("show");
        ep["SeasonId"] = json!("s1");
        let rows = items_from_jellyfin(&ep, Uuid::nil(), "Alex", Uuid::nil());
        assert_eq!(rows[0].item_type, "Episode");
        assert_eq!(
            (rows[0].parent_index_number, rows[0].index_number),
            (Some(2), Some(3))
        );
        assert_eq!(rows[0].extra_data["RemoteSeasonId"], "s1");
        assert!(!rows[0].is_folder);
    }

    #[test]
    fn each_version_is_a_copy_the_first_under_the_films_id() {
        let mut v = sample();
        let mut cut = v["MediaSources"][0].clone();
        cut["Id"] = json!("def");
        cut["Name"] = json!("1080p");
        cut["MediaStreams"][0]["Height"] = json!(800);
        v["MediaSources"].as_array_mut().unwrap().push(cut);
        let films = items_from_jellyfin(&v, Uuid::nil(), "Alex", Uuid::nil());
        let ids: Vec<&str> = films.iter().map(|f| f.remote_id.as_str()).collect();
        assert_eq!(ids, vec!["abc", "abc/def"]);
        assert_eq!(films[1].extra_data["RemoteSourceId"], "def");
        assert_eq!(films[1].extra_data["RemoteItemId"], "abc");
        assert_eq!(films[1].extra_data["LinkSource"], "1080p");
        assert_eq!(films[1].height, Some(800));
    }
}
