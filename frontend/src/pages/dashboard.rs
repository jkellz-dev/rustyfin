use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::dashboard_view;
use crate::types::{
    ActivityLogEntry, BaseItemDto, DashboardStats, LibraryStats, SessionInfo, StorageHealth,
    SystemInfo,
};

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
    // Auto-authenticate then load data
    spawn_local({
        async move {
            // Ensure we have a valid session token (auto-login via forward auth)
            if let Err(e) = api::ensure_authenticated().await {
                error.set(Some(e));
                loading.set(false);
                return;
            }

            let info_result = api::get_system_info().await;
            let stats_result = api::get_dashboard_stats().await;

            match info_result {
                Ok(i) => info.set(Some(i)),
                Err(e) => error.set(Some(e)),
            }
            if let Ok(s) = stats_result {
                stats.set(Some(s));
            }
            loading.set(false);
            // The widgets below fill in as they arrive; each may fail alone.
            if let Ok(s) = api::get_active_sessions().await {
                sessions.set(
                    s.into_iter()
                        .filter(|s| s.now_playing_item.is_some())
                        .collect(),
                );
            }
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
                    Err(e) => error.set(Some(e)),
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

            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else if let Some(err) = error.get_clone() {
                view! { div(class="card") { p(class="text-muted") { (err) } } }
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
                        let storage = health.storage.clone();
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
                                    list=move || storage.clone(),
                                    view=move |s: StorageHealth| {
                                        let dot = dashboard_view::health_class(&s.status);
                                        let detail = dashboard_view::storage_detail(&s.status, s.total_bytes, s.free_bytes);
                                        let title = s.path.clone();
                                        view! {
                                            div(class="health-row", title=title) {
                                                span(class=dot) {}
                                                span(class="health-name") { (s.label) }
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

                    // Now playing
                    (if sessions.with(|s| !s.is_empty()) {
                        view! {
                            h3(class="dashboard-heading") { "Now playing" }
                            div(class="card session-list") {
                                Indexed(
                                    list=move || sessions.get_clone(),
                                    view=move |s: SessionInfo| {
                                        let state = s.play_state.clone().unwrap_or_default();
                                        let title = s.now_playing_item.as_ref().map(|i| match &i.series_name {
                                            Some(series) => format!("{series}: {}", i.name),
                                            None => i.name.clone(),
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
                                        view! {
                                            div(class="session-row") {
                                                div { (line) }
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
                                            .and_then(|t| t.get("Primary").cloned())
                                            .unwrap_or_default();
                                        let src = format!("/Items/{}/Images/Primary?tag={tag}&maxWidth=160&quality=90", item.id);
                                        let label = match &item.series_name {
                                            Some(series) => format!("{series}: {}", item.name),
                                            None => item.name.clone(),
                                        };
                                        let shape = if item.item_type == "Episode" { "recent-art landscape" } else { "recent-art" };
                                        let alt = label.clone();
                                        view! {
                                            div(class="recent-card") {
                                                img(class=shape, src=src, alt=alt, loading="lazy")
                                                div(class="recent-title") { (label) }
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
