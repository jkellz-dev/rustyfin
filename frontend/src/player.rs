//! The admin's player (roadmap Phase 13): one media element for video and
//! music, held at the root of the app so music keeps playing while you move
//! between pages. Video opens over the page; music is a bar at the bottom.
//! What it decides lives in `player_logic`; this file talks to the browser.

use std::cell::RefCell;

use js_sys::{Array, Function, Promise, Reflect};
use serde_json::{Value, json};
use sycamore::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{HtmlMediaElement, HtmlSelectElement};

use crate::api;
use crate::browse;
use crate::player_logic::{self as logic, PlaySource, Queue, Shortcut, TrackChoice};

/// What is playing now.
#[derive(Debug, Clone, PartialEq)]
pub struct Now {
    pub item_id: String,
    pub name: String,
    /// Artist or show and episode.
    pub detail: String,
    pub is_audio: bool,
    pub image: Option<String>,
    pub play_session_id: String,
    pub media_source_id: String,
    pub play_method: &'static str,
    pub is_hls: bool,
    /// Where the transcode started: the element's 0 is this many seconds in.
    pub offset: f64,
    pub duration: f64,
    pub audio_tracks: Vec<TrackChoice>,
    pub subtitle_tracks: Vec<TrackChoice>,
    pub audio_index: Option<i32>,
    pub subtitle_index: Option<i32>,
    pub chapters: Vec<(f64, String, f64)>,
    /// Text subtitle tracks the browser shows itself (index, address).
    pub text_subtitles: Vec<(i32, String)>,
}

/// The player, shared through the app's context.
#[derive(Clone, Copy)]
pub struct Player {
    pub now: Signal<Option<Now>>,
    pub position: Signal<f64>,
    pub paused: Signal<bool>,
    pub queue: Signal<Queue>,
    pub error: Signal<Option<String>>,
    pub loading: Signal<bool>,
    seek_value: Signal<f64>,
    volume: Signal<f64>,
    muted: Signal<bool>,
    dragging: Signal<bool>,
}

thread_local! {
    static MEDIA: RefCell<Option<HtmlMediaElement>> = const { RefCell::new(None) };
    static HLS: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static LAST_REPORT: RefCell<f64> = const { RefCell::new(0.0) };
    static PENDING_SEEK: RefCell<Option<f64>> = const { RefCell::new(None) };
}

fn media() -> Option<HtmlMediaElement> {
    MEDIA.with(|m| m.borrow().clone())
}

impl Player {
    pub fn new() -> Self {
        Player {
            now: create_signal(None),
            position: create_signal(0.0),
            paused: create_signal(true),
            queue: create_signal(Queue::default()),
            error: create_signal(None),
            loading: create_signal(false),
            seek_value: create_signal(0.0),
            volume: create_signal(1.0),
            muted: create_signal(false),
            dragging: create_signal(false),
        }
    }

    /// Play `start` with `ids` as the queue (an album's tracks, or just one).
    pub fn play_queue(self, ids: Vec<String>, start: &str) {
        let queue = Queue::new(ids, start);
        let first = queue.current().map(str::to_string);
        self.queue.set(queue);
        if let Some(id) = first {
            spawn_local(async move { self.start(id, None, None, None).await });
        }
    }

    /// Play one item; a track plays with the rest of its album after it.
    pub fn play(self, item_id: String) {
        spawn_local(async move {
            let item = match api::player_get(&format!("/Items/{item_id}")).await {
                Ok(item) => item,
                Err(e) => {
                    self.error.set(Some(e.to_string()));
                    return;
                }
            };
            let ids = match item["Type"].as_str() {
                Some("MusicAlbum") => album_tracks(&item_id).await,
                Some("Audio") => match item["AlbumId"].as_str().or(item["ParentId"].as_str()) {
                    Some(album) => album_tracks(album).await,
                    None => vec![item_id.clone()],
                },
                _ => vec![item_id.clone()],
            };
            let ids = if ids.is_empty() {
                vec![item_id.clone()]
            } else {
                ids
            };
            let start = if item["Type"] == "MusicAlbum" {
                ids[0].clone()
            } else {
                item_id.clone()
            };
            self.play_queue(ids, &start);
        });
    }

    async fn start(self, item_id: String, at: Option<f64>, audio: Option<i32>, sub: Option<i32>) {
        self.loading.set(true);
        self.error.set(None);
        let same_item = self
            .now
            .with(|n| n.as_ref().is_some_and(|n| n.item_id == item_id));
        if !same_item {
            self.finish_current().await;
        } else {
            self.drop_stream().await;
        }
        let item = match api::player_get(&format!("/Items/{item_id}")).await {
            Ok(v) => v,
            Err(e) => return self.fail(e.to_string()),
        };
        let is_audio = item["Type"] == "Audio";
        let streams: Vec<Value> = item["MediaStreams"].as_array().cloned().unwrap_or_default();
        let audio_tracks = logic::track_choices(&streams, "Audio");
        let subtitle_tracks = logic::track_choices(&streams, "Subtitle");
        let default_audio = audio_tracks
            .iter()
            .find(|t| t.is_default)
            .or(audio_tracks.first())
            .map(|t| t.index);
        let start_at = at.unwrap_or_else(|| {
            logic::resume_secs(
                item["UserData"]["PlaybackPositionTicks"].as_i64(),
                item["UserData"]["Played"].as_bool().unwrap_or(false),
            )
        });
        let image_sub = sub.is_some_and(|i| {
            streams
                .iter()
                .any(|s| s["Index"].as_i64() == Some(i64::from(i)) && !is_text_subtitle(s))
        });
        let other_audio = audio.is_some() && audio != default_audio;
        let mut body = json!({
            "DeviceProfile": logic::browser_profile(),
            "StartTimeTicks": logic::secs_to_ticks(start_at),
            "MaxStreamingBitrate": 120_000_000,
            "EnableDirectPlay": !(image_sub || other_audio),
            "EnableDirectStream": !(image_sub || other_audio),
            "EnableTranscoding": true,
            "AutoOpenLiveStream": false,
        });
        if let Some(a) = audio {
            body["AudioStreamIndex"] = json!(a);
        }
        if let Some(s) = sub.filter(|_| image_sub) {
            body["SubtitleStreamIndex"] = json!(s);
        }
        let info = match api::playback_info(&item_id, &body).await {
            Ok(v) => v,
            Err(e) => return self.fail(e.to_string()),
        };
        let play_session_id = info["PlaySessionId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let Some(source) = info["MediaSources"]
            .as_array()
            .and_then(|s| s.first())
            .cloned()
        else {
            return self.fail("The server offered no way to play this.".into());
        };
        let token = api::get_token().unwrap_or_default();
        let Some(chosen) =
            logic::choose_source(&source, &item_id, is_audio, &play_session_id, &token)
        else {
            return self.fail("This file cannot be played in a browser.".into());
        };
        let media_source_id = source["Id"].as_str().unwrap_or(&item_id).to_string();
        let base = api::base_url();
        let (url, is_hls, play_method) = match &chosen {
            PlaySource::Direct(u) => (format!("{base}{u}"), false, "DirectPlay"),
            PlaySource::Hls(u) => (format!("{base}{u}"), true, "Transcode"),
        };
        let text_subtitles = if is_audio {
            Vec::new()
        } else {
            streams
                .iter()
                .filter(|s| s["Type"] == "Subtitle" && is_text_subtitle(s))
                .filter_map(|s| s["Index"].as_i64())
                .map(|i| {
                    (
                        i as i32,
                        format!("{base}/Videos/{item_id}/{media_source_id}/Subtitles/{i}/Stream.vtt?api_key={token}"),
                    )
                })
                .collect()
        };
        let detail = if is_audio {
            [item["AlbumArtist"].as_str(), item["Album"].as_str()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            match (
                item["SeriesName"].as_str(),
                item["ParentIndexNumber"].as_i64(),
                item["IndexNumber"].as_i64(),
            ) {
                (Some(show), Some(s), Some(e)) => format!("{show} · S{s:02}E{e:02}"),
                (Some(show), _, _) => show.to_string(),
                _ => item["ProductionYear"]
                    .as_i64()
                    .map(|y| y.to_string())
                    .unwrap_or_default(),
            }
        };
        let image_id = if is_audio {
            item["AlbumId"]
                .as_str()
                .filter(|_| item["ImageTags"]["Primary"].is_null())
                .unwrap_or(&item_id)
        } else {
            &item_id
        };
        let image = Some(browse::image_url(&base, image_id, None, 120, &token));
        let now = Now {
            item_id: item_id.clone(),
            name: item["Name"].as_str().unwrap_or("Untitled").to_string(),
            detail,
            is_audio,
            image,
            play_session_id: play_session_id.clone(),
            media_source_id: media_source_id.clone(),
            play_method,
            is_hls,
            offset: if is_hls { start_at } else { 0.0 },
            duration: item["RunTimeTicks"]
                .as_i64()
                .map(logic::ticks_to_secs)
                .unwrap_or(0.0),
            audio_tracks,
            subtitle_tracks,
            audio_index: audio.or(default_audio),
            subtitle_index: sub,
            chapters: logic::chapter_marks(
                item["Chapters"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                item["RunTimeTicks"].as_i64(),
            ),
            text_subtitles,
        };
        self.position.set(start_at);
        self.seek_value.set(start_at);
        let shown_sub = now.subtitle_index.and_then(|i| {
            now.text_subtitles
                .iter()
                .find(|(t, _)| *t == i)
                .map(|(_, u)| u.clone())
        });
        self.now.set(Some(now));
        let Some(el) = media() else { return };
        set_subtitle(&el, shown_sub.as_deref());
        if is_hls {
            if !el.can_play_type("application/vnd.apple.mpegurl").is_empty() {
                el.set_src(&url);
            } else if let Err(e) = attach_hls(&el, &url).await {
                return self.fail(e);
            }
        } else {
            PENDING_SEEK.with(|p| *p.borrow_mut() = (start_at > 0.0).then_some(start_at));
            el.set_src(&url);
        }
        let _ = el.play();
        LAST_REPORT.with(|r| *r.borrow_mut() = js_sys::Date::now());
        self.loading.set(false);
        let _ = api::report_playback("", &self.report(false)).await;
    }

    fn fail(self, message: String) {
        self.loading.set(false);
        self.error.set(Some(message));
    }

    fn report(self, paused: bool) -> Value {
        self.now.with(|n| match n {
            Some(n) => logic::report_body(
                &n.item_id,
                &n.play_session_id,
                &n.media_source_id,
                n.play_method,
                self.position.get(),
                paused,
            ),
            None => json!({}),
        })
    }

    /// Detach the stream (and end its transcode) without reporting a stop.
    async fn drop_stream(self) {
        let session = self.now.with(|n| {
            n.as_ref()
                .filter(|n| n.is_hls)
                .map(|n| n.play_session_id.clone())
        });
        destroy_hls();
        if let Some(el) = media() {
            let _ = el.pause();
            el.remove_attribute("src").ok();
            el.load();
        }
        if let Some(s) = session {
            let _ = api::stop_encoding(&s).await;
        }
    }

    /// Report the stop of what is playing and let go of it.
    async fn finish_current(self) {
        if self.now.with(Option::is_none) {
            return;
        }
        let body = self.report(true);
        self.drop_stream().await;
        let _ = api::report_playback("Stopped", &body).await;
    }

    /// Close the player.
    pub fn close(self) {
        spawn_local(async move {
            self.finish_current().await;
            self.now.set(None);
            self.queue.set(Queue::default());
            self.error.set(None);
            self.loading.set(false);
            exit_fullscreen();
        });
    }

    /// Jump to `secs` into the item. A transcode only reaches what it has
    /// made so far, so a jump past that starts it again from there.
    pub fn seek(self, secs: f64) {
        let Some(el) = media() else { return };
        let Some((offset, is_hls, duration, item, audio, sub)) = self.now.with(|n| {
            n.as_ref().map(|n| {
                (
                    n.offset,
                    n.is_hls,
                    n.duration,
                    n.item_id.clone(),
                    n.audio_index,
                    n.subtitle_index,
                )
            })
        }) else {
            return;
        };
        let target = if duration > 0.0 {
            secs.clamp(0.0, duration)
        } else {
            secs.max(0.0)
        };
        let rel = target - offset;
        let reachable = !is_hls || (rel >= 0.0 && rel <= seekable_end(&el));
        if reachable {
            el.set_current_time(rel);
            self.position.set(target);
        } else {
            spawn_local(async move { self.start(item, Some(target), audio, sub).await });
        }
    }

    fn toggle(self) {
        if let Some(el) = media() {
            if el.paused() {
                let _ = el.play();
            } else {
                let _ = el.pause();
            }
        }
    }

    fn step(self, forward: bool) {
        let next = self.queue.with_untracked(|q| {
            let mut q = q.clone();
            let id = if forward { q.next() } else { q.previous() }.map(str::to_string);
            (q, id)
        });
        if let (q, Some(id)) = next {
            self.queue.set(q);
            spawn_local(async move { self.start(id, Some(0.0), None, None).await });
        }
    }

    fn change_tracks(self, audio: Option<i32>, sub: Option<i32>) {
        let Some((item, is_hls, current_audio, text)) = self.now.with(|n| {
            n.as_ref().map(|n| {
                (
                    n.item_id.clone(),
                    n.is_hls,
                    n.audio_index,
                    n.text_subtitles.clone(),
                )
            })
        }) else {
            return;
        };
        let at = self.position.get_untracked();
        // A text subtitle on a file played directly is shown by the browser.
        let text_only = audio == current_audio
            && !is_hls
            && sub.is_none_or(|s| text.iter().any(|(i, _)| *i == s));
        if text_only {
            if let Some(el) = media() {
                let url =
                    sub.and_then(|s| text.iter().find(|(i, _)| *i == s).map(|(_, u)| u.clone()));
                set_subtitle(&el, url.as_deref());
            }
            self.now.update(|n| {
                if let Some(n) = n {
                    n.subtitle_index = sub;
                }
            });
        } else {
            spawn_local(async move { self.start(item, Some(at), audio, sub).await });
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Player::new()
    }
}

async fn album_tracks(album_id: &str) -> Vec<String> {
    let path = format!(
        "/Items?ParentId={album_id}&IncludeItemTypes=Audio&Recursive=true&SortBy=ParentIndexNumber,IndexNumber,SortName&Fields=MediaStreams"
    );
    match api::player_get(&path).await {
        Ok(v) => v["Items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|i| i["Id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn is_text_subtitle(stream: &Value) -> bool {
    let codec = stream["Codec"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        codec.as_str(),
        "subrip" | "srt" | "webvtt" | "vtt" | "mov_text" | "text" | "ass" | "ssa"
    )
}

fn seekable_end(el: &HtmlMediaElement) -> f64 {
    let ranges = el.seekable();
    let n = ranges.length();
    if n == 0 {
        return 0.0;
    }
    ranges.end(n - 1).unwrap_or(0.0)
}

fn set_subtitle(el: &HtmlMediaElement, url: Option<&str>) {
    while let Some(child) = el.first_element_child() {
        child.remove();
    }
    let Some(url) = url else { return };
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if let Ok(track) = doc.create_element("track") {
        let _ = track.set_attribute("kind", "subtitles");
        let _ = track.set_attribute("src", url);
        let _ = track.set_attribute("default", "");
        let _ = el.append_child(&track);
    }
}

fn destroy_hls() {
    if let Some(hls) = HLS.with(|h| h.borrow_mut().take())
        && let Ok(f) = Reflect::get(&hls, &"destroy".into())
        && let Some(f) = f.dyn_ref::<Function>()
    {
        let _ = f.call0(&hls);
    }
}

/// hls.js is only fetched the first time a transcode plays (it is 120 KB
/// gzipped and most plays are direct).
async fn load_hls_js() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or("no window")?;
    let existing = Reflect::get(&window, &"Hls".into()).unwrap_or(JsValue::UNDEFINED);
    if !existing.is_undefined() {
        return Ok(existing);
    }
    let doc = window.document().ok_or("no document")?;
    let script = doc
        .create_element("script")
        .map_err(|_| "could not load the HLS player")?;
    let _ = script.set_attribute("src", &format!("{}/web/hls.light.min.js", api::base_url()));
    let promise = Promise::new(&mut |resolve, reject| {
        let _ = script.add_event_listener_with_callback("load", &resolve);
        let _ = script.add_event_listener_with_callback("error", &reject);
    });
    doc.head()
        .ok_or("no head")?
        .append_child(&script)
        .map_err(|_| "could not load the HLS player")?;
    JsFuture::from(promise)
        .await
        .map_err(|_| "could not load the HLS player".to_string())?;
    Reflect::get(&window, &"Hls".into()).map_err(|_| "could not load the HLS player".to_string())
}

async fn attach_hls(el: &HtmlMediaElement, url: &str) -> Result<(), String> {
    let ctor = load_hls_js().await?;
    let supported = Reflect::get(&ctor, &"isSupported".into())
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .and_then(|f| f.call0(&ctor).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !supported {
        return Err("This browser cannot play streamed video.".into());
    }
    let ctor: Function = ctor.dyn_into().map_err(|_| "HLS player missing")?;
    let hls = Reflect::construct(&ctor, &Array::new()).map_err(|_| "HLS player failed")?;
    let call = |name: &str, arg: &JsValue| {
        Reflect::get(&hls, &name.into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .map(|f| f.call1(&hls, arg))
    };
    call("loadSource", &JsValue::from_str(url));
    call("attachMedia", el.as_ref());
    HLS.with(|h| *h.borrow_mut() = Some(hls));
    Ok(())
}

fn exit_fullscreen() {
    if let Some(doc) = web_sys::window().and_then(|w| w.document())
        && doc.fullscreen_element().is_some()
    {
        doc.exit_fullscreen();
    }
}

fn toggle_fullscreen(shell: &web_sys::Element) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if doc.fullscreen_element().is_some() {
        doc.exit_fullscreen();
    } else {
        let _ = shell.request_fullscreen();
    }
}

fn listen(el: &web_sys::EventTarget, event: &str, f: impl FnMut(web_sys::Event) + 'static) {
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let _ = el.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref());
    cb.forget();
}

/// Wire the element's events to the player (once: the host never goes away).
fn attach(p: Player, el: HtmlMediaElement, shell: web_sys::Element) {
    MEDIA.with(|m| *m.borrow_mut() = Some(el.clone()));
    let target: &web_sys::EventTarget = el.as_ref();
    {
        let el2 = el.clone();
        listen(target, "loadedmetadata", move |_| {
            if let Some(at) = PENDING_SEEK.with(|s| s.borrow_mut().take()) {
                el2.set_current_time(at);
            }
        });
    }
    {
        let el2 = el.clone();
        listen(target, "timeupdate", move |_| {
            let offset = p
                .now
                .with_untracked(|n| n.as_ref().map(|n| n.offset).unwrap_or(0.0));
            let pos = offset + el2.current_time();
            p.position.set(pos);
            if !p.dragging.get_untracked() {
                p.seek_value.set(pos);
            }
            let now = js_sys::Date::now();
            let due = LAST_REPORT.with(|r| now - *r.borrow() >= logic::REPORT_EVERY_MS);
            if due && !el2.paused() {
                LAST_REPORT.with(|r| *r.borrow_mut() = now);
                let body = p.report(false);
                spawn_local(async move {
                    let _ = api::report_playback("Progress", &body).await;
                });
            }
        });
    }
    for (event, paused) in [("play", false), ("pause", true)] {
        listen(target, event, move |_| {
            p.paused.set(paused);
            if p.now.with_untracked(Option::is_some) {
                let body = p.report(paused);
                spawn_local(async move {
                    let _ = api::report_playback("Progress", &body).await;
                });
            }
        });
    }
    listen(target, "ended", move |_| {
        if p.queue.with_untracked(Queue::has_next) {
            p.step(true);
        } else {
            p.close();
        }
    });
    listen(target, "error", move |_| {
        if p.now.with_untracked(Option::is_some) && HLS.with(|h| h.borrow().is_none()) {
            p.fail("The file could not be played.".into());
        }
    });
    // Leaving the admin altogether still reports the stop.
    if let Some(window) = web_sys::window() {
        listen(window.as_ref(), "pagehide", move |_| {
            if p.now.with_untracked(Option::is_some) {
                let body = p.report(true);
                spawn_local(async move {
                    let _ = api::report_playback("Stopped", &body).await;
                });
            }
        });
        // Keyboard shortcuts while a video is open (not while typing).
        listen(window.as_ref(), "keydown", move |e| {
            let Some(e) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
                return;
            };
            let video_open = p
                .now
                .with_untracked(|n| n.as_ref().is_some_and(|n| !n.is_audio));
            if !video_open || e.ctrl_key() || e.meta_key() || e.alt_key() {
                return;
            }
            let typing = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element())
                .is_some_and(|a| matches!(a.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
            if typing && e.key() != "Escape" {
                return;
            }
            let Some(action) = logic::shortcut(&e.key()) else {
                return;
            };
            e.prevent_default();
            e.stop_propagation();
            match action {
                Shortcut::TogglePlay => p.toggle(),
                Shortcut::Seek(by) => p.seek(p.position.get_untracked() + by),
                Shortcut::Mute => p.muted.set(!p.muted.get_untracked()),
                Shortcut::Fullscreen => toggle_fullscreen(&shell),
                Shortcut::Close => p.close(),
            }
        });
    }
    create_effect(move || {
        let (volume, muted) = (p.volume.get(), p.muted.get());
        if let Some(el) = media() {
            el.set_volume(volume.clamp(0.0, 1.0));
            el.set_muted(muted);
        }
    });
}

fn select_value(e: &web_sys::Event) -> Option<i32> {
    e.target()
        .and_then(|t| t.dyn_into::<HtmlSelectElement>().ok())
        .and_then(|s| s.value().parse().ok())
}

#[component]
pub fn PlayerHost() -> View {
    let p = use_context::<Player>();
    let media_ref = create_node_ref();
    let shell_ref = create_node_ref();
    on_mount(move || {
        let (Ok(el), Ok(shell)) = (
            media_ref.get().dyn_into::<HtmlMediaElement>(),
            shell_ref.get().dyn_into::<web_sys::Element>(),
        ) else {
            return;
        };
        attach(p, el, shell);
    });
    let class = move || match p.now.with(|n| n.as_ref().map(|n| n.is_audio)) {
        None => "player player-off",
        Some(true) => "player player-audio",
        Some(false) => "player player-video",
    };
    let field =
        move |f: fn(&Now) -> String| move || p.now.with(|n| n.as_ref().map(f).unwrap_or_default());
    let duration = move || {
        p.now
            .with(|n| n.as_ref().map(|n| n.duration).unwrap_or(0.0))
    };
    let time = move || {
        format!(
            "{} / {}",
            logic::clock(p.position.get()),
            logic::clock(duration())
        )
    };
    let image = move || {
        p.now
            .with(|n| n.as_ref().and_then(|n| n.image.clone()).unwrap_or_default())
    };
    let has_queue = move || p.queue.with(|q| q.ids.len() > 1);
    let audio_tracks = move || {
        p.now.with(|n| {
            n.as_ref()
                .map(|n| n.audio_tracks.clone())
                .unwrap_or_default()
        })
    };
    let subtitle_tracks = move || {
        p.now.with(|n| {
            n.as_ref()
                .map(|n| n.subtitle_tracks.clone())
                .unwrap_or_default()
        })
    };
    let audio_index = move || p.now.with(|n| n.as_ref().and_then(|n| n.audio_index));
    let subtitle_index = move || p.now.with(|n| n.as_ref().and_then(|n| n.subtitle_index));
    let chapters = move || {
        p.now
            .with(|n| n.as_ref().map(|n| n.chapters.clone()).unwrap_or_default())
    };

    view! {
        div(class=class, r#ref=shell_ref, role="region", aria-label="Player") {
            video(r#ref=media_ref, class="player-media", playsinline=true, preload="auto") {}
            (if p.loading.get() {
                view! { div(class="player-loading", aria-live="polite") { span(class="spinner") {} " Starting…" } }
            } else {
                view! {}
            })
            (match p.error.get_clone() {
                Some(e) => view! { div(class="player-error", role="alert") { (e) } },
                None => view! {},
            })
            div(class="player-bar") {
                div(class="player-info") {
                    img(class="player-art", src=image, alt="")
                    div(class="player-titles") {
                        div(class="player-title") { (field(|n| n.name.clone())()) }
                        div(class="player-detail") { (field(|n| n.detail.clone())()) }
                    }
                }
                div(class="player-controls") {
                    (if has_queue() {
                        view! {
                            button(class="player-btn", aria-label="Previous", on:click=move |_| p.step(false)) { "⏮" }
                        }
                    } else {
                        view! {}
                    })
                    button(
                        class="player-btn player-play",
                        aria-label=move || if p.paused.get() { "Play" } else { "Pause" },
                        on:click=move |_| p.toggle()
                    ) {
                        (if p.paused.get() { "▶" } else { "⏸" })
                    }
                    (if has_queue() {
                        view! {
                            button(class="player-btn", aria-label="Next", on:click=move |_| p.step(true)) { "⏭" }
                        }
                    } else {
                        view! {}
                    })
                }
                div(class="player-timeline") {
                    div(class="player-seek-wrap") {
                        input(
                            r#type="range",
                            class="player-seek",
                            aria-label="Position",
                            min="0",
                            max=move || duration().max(1.0).to_string(),
                            step="1",
                            bind:valueAsNumber=p.seek_value,
                            on:input=move |_| p.dragging.set(true),
                            on:change=move |_| {
                                p.dragging.set(false);
                                p.seek(p.seek_value.get_untracked());
                            }
                        )
                        Indexed(
                            list=chapters,
                            view=move |(pct, name, at)| view! {
                                button(
                                    class="player-chapter",
                                    style=format!("left: {pct:.2}%"),
                                    title=name.clone(),
                                    aria-label=format!("Chapter: {name}"),
                                    on:click=move |_| p.seek(at)
                                ) {}
                            }
                        )
                    }
                    span(class="player-time") { (time()) }
                }
                div(class="player-extras") {
                    (if audio_tracks().len() > 1 {
                        view! {
                            select(
                                class="player-select",
                                aria-label="Audio track",
                                on:change=move |e: web_sys::Event| {
                                    p.change_tracks(select_value(&e), subtitle_index());
                                }
                            ) {
                                Indexed(
                                    list=audio_tracks,
                                    view=move |t| view! {
                                        option(value=t.index.to_string(), selected=audio_index() == Some(t.index)) { (t.label.clone()) }
                                    }
                                )
                            }
                        }
                    } else {
                        view! {}
                    })
                    (if !subtitle_tracks().is_empty() {
                        view! {
                            select(
                                class="player-select",
                                aria-label="Subtitles",
                                on:change=move |e: web_sys::Event| {
                                    p.change_tracks(audio_index(), select_value(&e));
                                }
                            ) {
                                option(value="none", selected=subtitle_index().is_none()) { "Subtitles off" }
                                Indexed(
                                    list=subtitle_tracks,
                                    view=move |t| view! {
                                        option(value=t.index.to_string(), selected=subtitle_index() == Some(t.index)) { (t.label.clone()) }
                                    }
                                )
                            }
                        }
                    } else {
                        view! {}
                    })
                    button(
                        class="player-btn",
                        aria-label=move || if p.muted.get() { "Unmute" } else { "Mute" },
                        on:click=move |_| p.muted.set(!p.muted.get())
                    ) {
                        (if p.muted.get() { "🔇" } else { "🔊" })
                    }
                    input(
                        r#type="range",
                        class="player-volume",
                        aria-label="Volume",
                        min="0",
                        max="1",
                        step="0.05",
                        bind:valueAsNumber=p.volume
                    )
                    button(
                        class="player-btn player-fullscreen",
                        aria-label="Full screen",
                        on:click=move |_| {
                            if let Ok(shell) = shell_ref.get().dyn_into::<web_sys::Element>() {
                                toggle_fullscreen(&shell);
                            }
                        }
                    ) { "⛶" }
                    button(class="player-btn", aria-label="Close player", on:click=move |_| p.close()) { "✕" }
                }
            }
        }
    }
}
