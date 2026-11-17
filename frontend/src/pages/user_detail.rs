//! A user's page (owner, 2026-11-17): how long they have watched or listened
//! to each kind of media, and a filterable list of every play.

use sycamore::prelude::*;
use sycamore_router::navigate;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::media_route::MediaRoute;
use crate::types::{DeviceInfo, PlayEntry, PlayStats, UserDto};
use crate::user_view;

const PAGE_SIZE: i32 = 50;

/// A date and time in the browser's own format.
fn local_time(iso: &str) -> String {
    let d = js_sys::Date::new(&JsValue::from_str(iso));
    if d.get_time().is_nan() {
        return iso.to_string();
    }
    d.to_locale_string("default", &JsValue::UNDEFINED).into()
}

#[component(inline_props)]
pub fn UserDetail(user_id: String) -> View {
    let user: Signal<Option<UserDto>> = create_signal(None);
    let stats: Signal<Option<PlayStats>> = create_signal(None);
    let plays: Signal<Vec<PlayEntry>> = create_signal(Vec::new());
    let total = create_signal(0i32);
    let start = create_signal(0i32);
    let kind = create_signal(String::new());
    let span_days = create_signal(0u32);
    let search = create_signal(String::new());
    let loading = create_signal(true);
    let devices: Signal<Vec<DeviceInfo>> = create_signal(Vec::new());
    // The device waiting for "Sign out" to be confirmed.
    let sign_out_target: Signal<Option<DeviceInfo>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);
    let error: Signal<Option<String>> = create_signal(None);

    let uid = create_signal(user_id.clone());
    let load_page = move || {
        let since = match span_days.get() {
            0 => None,
            d => Some(js_sys::Date::now() - f64::from(d) * 86_400_000.0),
        };
        let query = user_view::history_query(
            &uid.get_clone(),
            start.get(),
            PAGE_SIZE,
            &kind.get_clone(),
            &search.get_clone(),
            since,
        );
        loading.set(true);
        spawn_local(async move {
            match api::get_play_history(&query).await {
                Ok(result) => {
                    total.set(result.total_record_count);
                    plays.set(result.items);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };
    let refilter = move || {
        start.set(0);
        load_page();
    };

    {
        let uid = user_id.clone();
        spawn_local(async move {
            if let Err(e) = api::ensure_authenticated().await {
                error.set(Some(e));
                return;
            }
            match api::get_user(&uid).await {
                Ok(u) => user.set(Some(u)),
                Err(e) => error.set(Some(e)),
            }
            if let Ok(s) = api::get_play_stats(&uid).await {
                stats.set(Some(s));
            }
            if let Ok(d) = api::get_devices(&uid).await {
                devices.set(d.items);
            }
            load_page();
        });
    }

    let confirm_sign_out = move |_| {
        if let Some(device) = sign_out_target.get_clone() {
            spawn_local(async move {
                match api::delete_device(&device.id).await {
                    Ok(()) => {
                        devices.update(|list| list.retain(|d| d.id != device.id));
                        toast.set(Some(format!("Signed out {}", device.name)));
                    }
                    Err(e) => error.set(Some(e)),
                }
                sign_out_target.set(None);
            });
        }
    };

    view! {
        div {
            (if let Some(msg) = toast.get_clone() {
                let shown = msg.clone();
                spawn_local(async move {
                    crate::timer::sleep_ms(crate::timer::TOAST_MS).await;
                    if toast.get_clone().as_deref() == Some(shown.as_str()) {
                        toast.set(None);
                    }
                });
                view! { div(class="toast-container") { div(class="toast toast-success") { (msg) } } }
            } else {
                view! {}
            })
            (if let Some(target) = sign_out_target.get_clone() {
                let label = format!("{} ({})", target.name, target.app_name);
                view! {
                    div(class="modal-overlay") {
                        div(class="modal") {
                            h3 { "Sign out device" }
                            p { "Sign out " (label) "? The app will have to sign in again; anything playing on it stops reporting." }
                            div(class="modal-actions") {
                                button(class="btn btn-secondary", on:click=move |_| sign_out_target.set(None)) { "Cancel" }
                                button(class="btn btn-danger", on:click=confirm_sign_out) { "Sign out" }
                            }
                        }
                    }
                }
            } else {
                view! {}
            })
            div(class="page-header") {
                h1 { (user.with(|u| u.as_ref().map(|u| u.name.clone()).unwrap_or_else(|| "User".to_string()))) }
                a(class="btn btn-secondary btn-sm", href="/web/users", on:click=move |e: web_sys::MouseEvent| {
                    e.prevent_default();
                    navigate("/web/users");
                }) { "All users" }
            }

            (if let Some(err) = error.get_clone() {
                view! { div(class="card mb-16") { p(class="text-muted") { (err) } } }
            } else {
                view! {}
            })

            // Time per kind of media
            (if let Some(s) = stats.get_clone() {
                let kinds: Vec<(String, f64, i64)> = ["Movie", "Episode", "Audio", "Other"]
                    .iter()
                    .map(|k| {
                        let found = s.by_type.iter().find(|t| t.kind == *k);
                        (
                            user_view::kind_label(k).to_string(),
                            found.map_or(0.0, |t| t.minutes),
                            found.map_or(0, |t| t.plays),
                        )
                    })
                    // "Other" only when there is some.
                    .filter(|(label, _, plays)| label != "Other" || *plays > 0)
                    .collect();
                let total_label = user_view::duration_label(s.total_minutes);
                let total_plays = s.plays;
                view! {
                    div(class="stat-grid") {
                        div(class="stat-card") {
                            div(class="stat-value") { (total_label) }
                            div(class="stat-label") { "In total · " (total_plays) " plays" }
                        }
                        Indexed(
                            list=move || kinds.clone(),
                            view=move |(label, minutes, count): (String, f64, i64)| {
                                let value = user_view::duration_label(minutes);
                                view! {
                                    div(class="stat-card") {
                                        div(class="stat-value") { (value) }
                                        div(class="stat-label") { (label) " · " (count) " plays" }
                                    }
                                }
                            }
                        )
                    }
                }
            } else {
                view! {}
            })

            // Devices signed in (roadmap 12e)
            h3(class="dashboard-heading") { "Devices" }
            (if devices.with(Vec::is_empty) {
                view! { div(class="card text-muted") { "No devices signed in." } }
            } else {
                let now = js_sys::Date::now();
                let list = devices.get_clone();
                view! {
                    div(class="card history-list") {
                        Indexed(
                            list=move || list.clone(),
                            view=move |d: DeviceInfo| {
                                let when = d
                                    .date_last_activated
                                    .as_deref()
                                    .map(|s| js_sys::Date::new(&JsValue::from_str(s)).get_time())
                                    .filter(|t| !t.is_nan())
                                    .map(|t| user_view::ago_label(now, t))
                                    .unwrap_or_default();
                                let app = match d.app_version.as_deref() {
                                    Some(v) if !v.is_empty() => format!("{} {v}", d.app_name),
                                    _ => d.app_name.clone(),
                                };
                                let name = d.name.clone();
                                let target = d.clone();
                                view! {
                                    div(class="history-row") {
                                        div(class="history-main") {
                                            div(class="history-title") { (name) }
                                            div(class="text-muted history-detail") { (app) " · active " (when) }
                                        }
                                        div(class="history-side") {
                                            button(class="btn btn-secondary btn-sm", on:click=move |_| sign_out_target.set(Some(target.clone()))) { "Sign out" }
                                        }
                                    }
                                }
                            }
                        )
                    }
                }
            })

            // History, with filters
            h3(class="dashboard-heading") { "Watched and listened" }
            div(class="history-filters") {
                select(on:change=move |e: web_sys::Event| {
                    let value = e
                        .target()
                        .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok())
                        .map(|s| s.value())
                        .unwrap_or_default();
                    kind.set(value);
                    refilter();
                }) {
                    Indexed(
                        list=move || user_view::KINDS.to_vec(),
                        view=move |(value, label): (&'static str, &'static str)| view! {
                            option(value=value) { (label) }
                        }
                    )
                }
                select(on:change=move |e: web_sys::Event| {
                    let value = e
                        .target()
                        .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok())
                        .and_then(|s| s.value().parse().ok())
                        .unwrap_or(0);
                    span_days.set(value);
                    refilter();
                }) {
                    Indexed(
                        list=move || user_view::SPANS.to_vec(),
                        view=move |(days, label): (u32, &'static str)| view! {
                            option(value=days.to_string()) { (label) }
                        }
                    )
                }
                input(
                    class="search-input",
                    r#type="search",
                    placeholder="Title, show or artist",
                    bind:value=search,
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        if e.key() == "Enter" {
                            refilter();
                        }
                    },
                )
            }

            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else if plays.with(Vec::is_empty) {
                view! { div(class="empty-state") { "Nothing played yet." } }
            } else {
                let rows = plays.get_clone();
                view! {
                    div(class="card history-list") {
                        Indexed(
                            list=move || rows.clone(),
                            view=move |p: PlayEntry| {
                                let when = local_time(&p.started_at);
                                let length = user_view::duration_label(p.minutes);
                                let detail = user_view::play_detail(
                                    p.parent_name.as_deref(),
                                    p.client.as_deref(),
                                    p.device_name.as_deref(),
                                    p.play_method.as_deref(),
                                );
                                let kind = user_view::kind_label(&p.kind);
                                let name = p.name.clone();
                                let title = match p.item_id.clone() {
                                    Some(id) => {
                                        let path = MediaRoute::Item { id }.path();
                                        let href = path.clone();
                                        view! {
                                            a(href=href, on:click=move |e: web_sys::MouseEvent| {
                                                e.prevent_default();
                                                navigate(&path);
                                            }) { (name) }
                                        }
                                    }
                                    None => view! { span { (name) } },
                                };
                                let live = p.in_progress;
                                let shape = user_view::picture_shape(
                                    &p.kind,
                                    p.item_id.as_deref(),
                                    p.image_item_id.as_deref(),
                                );
                                let picture = match p.image_item_id.clone() {
                                    Some(id) => {
                                        let token = api::get_token().unwrap_or_default();
                                        let src = crate::browse::image_url(
                                            &api::base_url(),
                                            &id,
                                            p.image_tag.as_deref(),
                                            crate::browse::THUMB_IMAGE_WIDTH,
                                            &token,
                                        );
                                        let class = format!("history-thumb {shape}");
                                        view! { img(class=class, src=src, alt="", loading="lazy") }
                                    }
                                    None => {
                                        let class = format!("history-thumb {shape} history-thumb-empty");
                                        view! { div(class=class) }
                                    }
                                };
                                view! {
                                    div(class="history-row") {
                                        (picture)
                                        div(class="history-main") {
                                            div(class="history-title") {
                                                (title)
                                                (if live {
                                                    view! { span(class="badge badge-live") { "Playing" } }
                                                } else {
                                                    view! {}
                                                })
                                            }
                                            div(class="text-muted history-detail") { (detail) }
                                        }
                                        div(class="history-side text-muted") {
                                            div { (kind) " · " (length) }
                                            div { (when) }
                                        }
                                    }
                                }
                            }
                        )
                    }
                    (if total.get() > PAGE_SIZE {
                        let s = start.get();
                        let t = total.get();
                        let shown_to = (s + PAGE_SIZE).min(t);
                        view! {
                            div(class="pagination") {
                                button(class="btn btn-secondary btn-sm", disabled=s == 0, on:click=move |_| {
                                    start.set((s - PAGE_SIZE).max(0));
                                    load_page();
                                }) { "Previous" }
                                span(class="pagination-info") { (s + 1) " - " (shown_to) " of " (t) }
                                button(class="btn btn-secondary btn-sm", disabled=s + PAGE_SIZE >= t, on:click=move |_| {
                                    start.set(s + PAGE_SIZE);
                                    load_page();
                                }) { "Next" }
                            }
                        }
                    } else {
                        view! {}
                    })
                }
            })
        }
    }
}
