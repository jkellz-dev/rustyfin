use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::{DashboardStats, SystemInfo};

#[component]
pub fn Dashboard() -> View {
    let info: Signal<Option<SystemInfo>> = create_signal(None);
    let stats: Signal<Option<DashboardStats>> = create_signal(None);
    let loading = create_signal(true);
    let scan_loading = create_signal(false);
    let error: Signal<Option<String>> = create_signal(None);

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
                }
            })
        }
    }
}
