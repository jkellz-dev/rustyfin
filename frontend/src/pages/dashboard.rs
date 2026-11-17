use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::components::error_notice::ErrorNotice;
use crate::dashboard_view;
use crate::media_route::MediaRoute;
use crate::timer::sleep_ms;
use crate::types::{
    ActivityLogEntry, BaseItemDto, CacheHealth, DashboardStats, LibraryHealth, LibraryStats,
    SessionInfo, SystemInfo,
};
use crate::user_view;
use sycamore_router::navigate;

/// Seconds since an ISO date, by the browser's clock (None if unparsable).
fn age_seconds(iso: &str) -> Option<i64> {
    let then = js_sys::Date::parse(iso);
    if then.is_nan() {
        return None;
    }
    Some(((js_sys::Date::now() - then) / 1000.0) as i64)
}

#[component]
pub fn Dashboard() -> View {
    let info: Signal<Option<SystemInfo>> = create_signal(None);
    let stats: Signal<Option<DashboardStats>> = create_signal(None);
    let loading = create_signal(true);
    let scan_loading = create_signal(false);
    let error: Signal<Option<String>> = create_signal(None);
    let activity: Signal<Vec<ActivityLogEntry>> = create_signal(Vec::new());
    let sessions: Signal<Vec<SessionInfo>> = create_signal(Vec::new());
    let latest: Signal<Vec<BaseItemDto>> = create_signal(Vec::new());
    // Streams refresh every 10 s while the dashboard is open (owner,
    // 2026-11-17: the dashboard lists current streams).
    let alive = std::rc::Rc::new(std::cell::Cell::new(true));
    {
        let alive = alive.clone();
        on_cleanup(move || alive.set(false));
    }
    let refresh_streams = move || async move {
        if let Ok(s) = api::get_active_sessions().await {
            sessions.set(
                s.into_iter()
                    .filter(|s| s.now_playing_item.is_some())
                    .collect(),
            );
        }
    };
    {
        let alive = alive.clone();
        spawn_local(async move {
            loop {
                sleep_ms(10_000).await;
                if !alive.get() {
                    break;
                }
                refresh_streams().await;
            }
        });
    }
    // Auto-authenticate then load data
    spawn_local({
        async move {
            // Ensure we have a valid session token (auto-login via forward auth)
            if let Err(e) = api::ensure_authenticated().await {
                error.set(Some(e.to_string()));
                loading.set(false);
                return;
            }

            let info_result = api::get_system_info().await;
            let stats_result = api::get_dashboard_stats().await;

            match info_result {
                Ok(i) => info.set(Some(i)),
                Err(e) => error.set(Some(e.to_string())),
            }
            if let Ok(s) = stats_result {
                stats.set(Some(s));
            }
            loading.set(false);
            // The widgets below fill in as they arrive; each may fail alone.
            refresh_streams().await;
            if let Ok(items) = api::get_latest_items(12).await {
                latest.set(items);
            }
            if let Ok(log) = api::get_activity_log(15).await {
                activity.set(log.items);
            }
        }
    });

    let on_scan_all = move |_| {
        scan_loading.set(true);
        spawn_local({
            async move {
                match api::scan_library(None).await {
                    Ok(()) => {}
                    Err(e) => error.set(Some(e.to_string())),
                }
                scan_loading.set(false);
            }
        });
    };

    view! {
        div {
            div(class="page-header") {
                h1 { "Dashboard" }
                button(class="btn btn-primary", on:click=on_scan_all, disabled=scan_loading.get()) {
                    (if scan_loading.get() { "Scanning..." } else { "Scan All Libraries" })
                }
            }

            ErrorNotice(error=error)
            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else {
                view! {
                    // Server info card
                    (if let Some(sys) = info.get_clone() {
                        view! {
                            div(class="card mb-24") {
                                h2 { (sys.server_name) }
                                p(class="text-muted") { "Version " (sys.version) }
                            }
                        }
                    } else {
                        view! {}
                    })

                    // Stats
                    (if let Some(st) = stats.get_clone() {
                        view! {
                            div(class="stat-grid") {
                                div(class="stat-card") {
                                    div(class="stat-value") { (st.library_count) }
                                    div(class="stat-label") { "Libraries" }
                                }
                                div(class="stat-card") {
                                    div(class="stat-value") { (st.user_count) }
                                    div(class="stat-label") { "Users" }
                                }
                                div(class="stat-card") {
                                    div(class="stat-value") { (st.item_count) }
                                    div(class="stat-label") { "Media Items" }
                                }
                            }
                        }
                    } else {
                        view! {
                            p(class="text-muted") { "Sign in to view statistics." }
                        }
                    })

                    // Streams
                    h3(class="dashboard-heading") {
                        "Streams"
                        span(class="text-muted dashboard-count") { (sessions.with(Vec::len)) }
                    }
                    (if sessions.with(|s| !s.is_empty()) {
                        view! {
                            div(class="card session-list") {
                                Indexed(
                                    list=move || sessions.get_clone(),
                                    view=move |s: SessionInfo| {
                                        let state = s.play_state.clone().unwrap_or_default();
                                        let title = s.now_playing_item.as_ref().map(|i| {
                                            dashboard_view::playing_title(
                                                &i.item_type,
                                                &i.name,
                                                i.series_name.as_deref(),
                                                i.parent_index_number,
                                                i.index_number,
                                            )
                                        });
                                        let line = dashboard_view::session_label(
                                            s.user_name.as_deref(),
                                            &s.client,
                                            &s.device_name,
                                            title.as_deref(),
                                            state.is_paused,
                                        );
                                        let pct = s
                                            .now_playing_item
                                            .as_ref()
                                            .and_then(|i| i.runtime_ticks)
                                            .zip(state.position_ticks)
                                            .filter(|(run, _)| *run > 0)
                                            .map(|(run, pos)| (pos as f64 * 100.0 / run as f64).clamp(0.0, 100.0));
                                        let how = user_view::stream_method_label(
                                            state.play_method.as_deref().unwrap_or_default(),
                                        );
                                        let item_path = s
                                            .now_playing_item
                                            .as_ref()
                                            .map(|i| MediaRoute::Item { id: i.id.clone() }.path());
                                        let title_view = match item_path {
                                            Some(path) => {
                                                let href = path.clone();
                                                view! {
                                                    a(href=href, on:click=move |e: web_sys::MouseEvent| {
                                                        e.prevent_default();
                                                        navigate(&path);
                                                    }) { (line) }
                                                }
                                            }
                                            None => view! { span { (line) } },
                                        };
                                        view! {
                                            div(class="session-row") {
                                                div(class="session-line") {
                                                    (title_view)
                                                    span(class="badge badge-type") { (how) }
                                                }
                                                (match pct {
                                                    Some(p) => {
                                                        let style = format!("width: {p:.1}%;");
                                                        view! { div(class="session-progress") { div(class="card-progress-fill", style=style) {} } }
                                                    }
                                                    None => view! {},
                                                })
                                            }
                                        }
                                    }
                                )
                            }
                        }
                    } else {
                        view! { div(class="card text-muted") { "No one is streaming." } }
                    })

                    // Library cards
                    (if let Some(st) = stats.get_clone().filter(|s| !s.libraries.is_empty()) {
                        let libs = st.libraries.clone();
                        view! {
                            h3(class="dashboard-heading") { "Libraries" }
                            div(class="library-cards") {
                                Indexed(
                                    list=move || libs.clone(),
                                    view=move |lib: LibraryStats| {
                                        let count = dashboard_view::count_noun(&lib.collection_type, lib.item_count);
                                        let scanned = dashboard_view::last_scan_label(
                                            lib.last_scanned.as_deref().and_then(age_seconds),
                                        );
                                        view! {
                                            div(class="library-card") {
                                                div(class="library-card-name") { (lib.name) }
                                                div(class="library-card-count") { (count) }
                                                div(class="library-card-scan text-muted") { (scanned) }
                                            }
                                        }
                                    }
                                )
                            }
                        }
                    } else {
                        view! {}
                    })

                    // System health
                    (if let Some(health) = stats.get_clone().and_then(|s| s.health) {
                        let db = health.database.clone();
                        let db_dot = dashboard_view::health_class(if db.ok { "Ok" } else { "Down" });
                        let db_text = if db.ok {
                            format!("{:.1} ms, {} of {} connections idle", db.latency_ms, db.pool_idle, db.pool_size)
                        } else {
                            db.error.clone().unwrap_or_else(|| "Not answering".to_string())
                        };
                        let search = health.search.clone();
                        let search_dot = dashboard_view::health_class(if search.ok { "Ok" } else { "Down" });
                        let search_text = match &search.detail {
                            Some(d) => format!("{}: {d}", search.backend),
                            None => search.backend.clone(),
                        };
                        let libraries = health.libraries.clone();
                        let caches = health.caches.clone();
                        view! {
                            h3(class="dashboard-heading") { "System health" }
                            div(class="card health-list") {
                                div(class="health-row") {
                                    span(class=db_dot) {}
                                    span(class="health-name") { "Database" }
                                    span(class="text-muted health-detail") { (db_text) }
                                }
                                div(class="health-row") {
                                    span(class=search_dot) {}
                                    span(class="health-name") { "Search" }
                                    span(class="text-muted health-detail") { (search_text) }
                                }
                                Indexed(
                                    list=move || libraries.clone(),
                                    view=move |l: LibraryHealth| {
                                        let dot = dashboard_view::health_class(&l.status);
                                        let folders: Vec<(String, String)> = l
                                            .folders
                                            .iter()
                                            .map(|f| (f.path.clone(), f.status.clone()))
                                            .collect();
                                        let detail = dashboard_view::library_detail(&l.status, &folders);
                                        let title = folders
                                            .iter()
                                            .map(|(p, _)| p.as_str())
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                        view! {
                                            div(class="health-row", title=title) {
                                                span(class=dot) {}
                                                span(class="health-name") { (l.name) }
                                                span(class="text-muted health-detail") { (detail) }
                                            }
                                        }
                                    }
                                )
                                Indexed(
                                    list=move || caches.clone(),
                                    view=move |c: CacheHealth| {
                                        let dot = dashboard_view::health_class(&c.status);
                                        let detail = dashboard_view::cache_detail(&c.status, c.size_bytes, c.file_count);
                                        let title = c.path.clone();
                                        view! {
                                            div(class="health-row", title=title) {
                                                span(class=dot) {}
                                                span(class="health-name") { (c.name) }
                                                span(class="text-muted health-detail") { (detail) }
                                            }
                                        }
                                    }
                                )
                            }
                        }
                    } else {
                        view! {}
                    })

                    // Recently added
                    (if latest.with(|l| !l.is_empty()) {
                        view! {
                            h3(class="dashboard-heading") { "Recently added" }
                            div(class="recent-row") {
                                Indexed(
                                    list=move || latest.get_clone(),
                                    view=move |item: BaseItemDto| {
                                        let tag = item
                                            .image_tags
                                            .as_ref()
                                            .and_then(|t| t.get("Primary").cloned());
                                        let card = dashboard_view::recent_card(
                                            &item.id,
                                            &item.item_type,
                                            &item.name,
                                            tag.as_deref(),
                                            item.series_name.as_deref(),
                                            item.album_artist.as_deref(),
                                        );
                                        let label = card.label.clone();
                                        let shape = card.shape;
                                        let art = match card.image {
                                            Some(src) => {
                                                let alt = label.clone();
                                                view! { img(class=shape, src=src, alt=alt, loading="lazy") }
                                            }
                                            None => {
                                                let placeholder = format!("{shape} recent-art-empty");
                                                let initial = label.chars().next().map(String::from).unwrap_or_default();
                                                view! { div(class=placeholder) { (initial) } }
                                            }
                                        };
                                        let path = MediaRoute::Item { id: item.id.clone() }.path();
                                        // The full name on hover; the tile shows it cut short.
                                        let full = label.clone();
                                        view! {
                                            div(class="recent-card", on:click=move |_| navigate(&path)) {
                                                (art)
                                                div(class="recent-title", title=full) { (label) }
                                            }
                                        }
                                    }
                                )
                            }
                        }
                    } else {
                        view! {}
                    })

                    // Activity
                    (if activity.with(|a| !a.is_empty()) {
                        view! {
                            h3(class="dashboard-heading") { "Activity" }
                            div(class="card activity-list") {
                                Indexed(
                                    list=move || activity.get_clone(),
                                    view=move |e: ActivityLogEntry| {
                                        let when = age_seconds(&e.date).map(dashboard_view::ago_label).unwrap_or_default();
                                        let dot = dashboard_view::severity_class(&e.severity);
                                        let overview = e.overview.clone().unwrap_or_default();
                                        view! {
                                            div(class="activity-row") {
                                                span(class=dot) {}
                                                div(class="activity-text") {
                                                    div { (e.name) }
                                                    (if overview.is_empty() {
                                                        view! {}
                                                    } else {
                                                        let o = overview.clone();
                                                        view! { div(class="text-muted activity-overview") { (o) } }
                                                    })
                                                }
                                                span(class="text-muted activity-when") { (when) }
                                            }
                                        }
                                    }
                                )
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
