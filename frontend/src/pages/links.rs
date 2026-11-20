//! Linked servers (roadmap 16b): each friend's server, how its last sync
//! went, what it brought, and a sync on demand.

use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::components::error_notice::ErrorNotice;
use crate::components::loading::Loading;
use crate::link_view::{LinkState, badge, failures_label, link_state, response_label, sync_took};
use crate::timer::sleep_ms;
use crate::types::LinkInfo;

#[component]
pub fn Links() -> View {
    api::new_page();
    let links: Signal<Vec<LinkInfo>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);
    let polling = create_signal(false);

    spawn_local(async move {
        match api::get_links().await {
            Ok(l) => {
                let busy = l.iter().any(|l| l.syncing);
                links.set(l);
                if busy {
                    start_polling(links, polling, toast);
                }
            }
            Err(e) => error.set(Some(e.to_string())),
        }
        loading.set(false);
    });

    let on_sync = move |id: String, name: String| {
        spawn_local(async move {
            match api::sync_link(&id).await {
                Ok(()) => {
                    // Show it busy at once; the poll brings the outcome.
                    links.update(|l| {
                        if let Some(link) = l.iter_mut().find(|l| l.id == id) {
                            link.syncing = true;
                        }
                    });
                    toast.set(Some(format!("Syncing {name}")));
                    start_polling(links, polling, toast);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    view! {
        div {
            (if let Some(msg) = toast.get_clone() {
                crate::timer::clear_toast_later(toast, msg.clone());
                view! {
                    div(class="toast-container", role="status", aria-live="polite") {
                        div(class="toast toast-success") { (msg) }
                    }
                }
            } else {
                view! {}
            })

            div(class="page-header") {
                h1 { "Linked Servers" }
            }
            ErrorNotice(error=error)

            (if loading.get() {
                view! { Loading(kind="rows") }
            } else if links.with(Vec::is_empty) {
                view! {
                    div(class="card") {
                        p(class="text-muted") {
                            "No linked servers. Add a [[link]] to the config file to merge a friend's libraries into yours (docs/linked-servers.md)."
                        }
                    }
                }
            } else {
                view! {
                    div(class="card") {
                        table {
                            thead {
                                tr {
                                    th { "Name" }
                                    th { "Server" }
                                    th { "State" }
                                    th { "Last sync" }
                                    th { "Titles" }
                                    th { "Copies" }
                                    th(title="Since this server started") { "Response" }
                                    th(title="Since this server started") { "Failures" }
                                    th { "Actions" }
                                }
                            }
                            tbody {
                                Indexed(
                                    list=move || links.get_clone(),
                                    view=move |link| {
                                        let state = link_state(&link);
                                        let (class, label) = badge(&state);
                                        let why = match &state {
                                            LinkState::Failed(e) => e.clone(),
                                            _ => String::new(),
                                        };
                                        let last = link
                                            .last_sync_at
                                            .as_deref()
                                            .map(format_datetime)
                                            .unwrap_or_else(|| "Never".into());
                                        let took = sync_took(link.last_sync_ms);
                                        let response = response_label(link.response_ms, link.slowest_ms);
                                        let failures = failures_label(link.failed_syncs, link.failed_requests);
                                        let id = link.id.clone();
                                        let name = link.name.clone();
                                        let can_sync = link.configured && !link.syncing;
                                        // Built once: the row is rebuilt whenever the list changes.
                                        let reason = if why.is_empty() {
                                            view! {}
                                        } else {
                                            let shown = why.clone();
                                            view! { br {} span(class="text-muted text-sm") { (shown) } }
                                        };
                                        view! {
                                            tr {
                                                td(data-label="Name") {
                                                    span { (link.name.clone()) }
                                                    br {}
                                                    span(class="text-muted text-sm") { (link.kind.clone()) }
                                                }
                                                td(class="text-muted", data-label="Server") { (link.url.clone()) }
                                                td(data-label="State") {
                                                    span(class=class, title=why) { (label) }
                                                    (reason)
                                                }
                                                td(class="text-muted", data-label="Last sync") {
                                                    (last)
                                                    br {}
                                                    span(class="text-sm") { (took) }
                                                }
                                                td(data-label="Titles") { (link.titles) }
                                                td(data-label="Copies") { (link.copies) }
                                                td(class="text-muted", data-label="Response") { (response) }
                                                td(class="text-muted", data-label="Failures") { (failures) }
                                                td(data-label="Actions") {
                                                    button(
                                                        class="btn btn-primary btn-sm",
                                                        disabled=!can_sync,
                                                        on:click=move |_| on_sync(id.clone(), name.clone())
                                                    ) { "Sync now" }
                                                }
                                            }
                                        }
                                    }
                                )
                            }
                        }
                    }
                }
            })
        }
    }
}

/// Poll every 2 s while a link syncs; say when each one finishes.
fn start_polling(
    links: Signal<Vec<LinkInfo>>,
    polling: Signal<bool>,
    toast: Signal<Option<String>>,
) {
    if polling.get() {
        return;
    }
    polling.set(true);
    spawn_local(async move {
        loop {
            sleep_ms(2000).await;
            let Ok(now) = api::get_links().await else {
                break;
            };
            let before = links.get_clone();
            for link in &now {
                let was = before.iter().any(|b| b.id == link.id && b.syncing);
                if was && !link.syncing {
                    let outcome = match link_state(link) {
                        LinkState::Failed(_) => "failed",
                        _ => "synced",
                    };
                    toast.set(Some(format!("{} {outcome}", link.name)));
                }
            }
            let busy = now.iter().any(|l| l.syncing);
            links.set(now);
            if !busy {
                break;
            }
        }
        polling.set(false);
    });
}

/// "YYYY-MM-DD HH:MM" from an RFC 3339 time.
fn format_datetime(rfc3339: &str) -> String {
    if rfc3339.len() >= 16 {
        rfc3339[..16].replace('T', " ")
    } else {
        rfc3339.to_string()
    }
}
