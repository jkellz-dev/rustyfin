use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::{CreateLibraryRequest, VirtualFolderInfo};

#[component]
pub fn Libraries() -> View {
    let libs: Signal<Vec<VirtualFolderInfo>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);

    // Form state
    let name = create_signal(String::new());
    let collection_type = create_signal("movies".to_string());
    let path_input = create_signal(String::new());
    let creating = create_signal(false);

    // Delete modal
    let delete_target: Signal<Option<VirtualFolderInfo>> = create_signal(None);

    let load_libs = move || {
        spawn_local({
            async move {
                match api::get_libraries().await {
                    Ok(l) => libs.set(l),
                    Err(e) => error.set(Some(e)),
                }
                loading.set(false);
            }
        });
    };

    load_libs();

    let on_create = move |_| {
        let n = name.get_clone();
        let ct = collection_type.get_clone();
        let p = path_input.get_clone();
        if n.is_empty() || p.is_empty() {
            return;
        }
        creating.set(true);
        spawn_local({
            async move {
                let req = CreateLibraryRequest {
                    name: n,
                    collection_type: ct,
                    paths: vec![p],
                };
                match api::create_library(&req).await {
                    Ok(lib) => {
                        libs.update(|l| l.push(lib));
                        name.set(String::new());
                        path_input.set(String::new());
                        toast.set(Some("Library created".to_string()));
                    }
                    Err(e) => error.set(Some(e)),
                }
                creating.set(false);
            }
        });
    };

    let on_scan = move |id: String| {
        spawn_local({
            async move {
                match api::scan_library(Some(&id)).await {
                    Ok(()) => toast.set(Some("Scan started".to_string())),
                    Err(e) => error.set(Some(e)),
                }
            }
        });
    };

    let confirm_delete = move |_| {
        if let Some(target) = delete_target.get_clone() {
            let id = target.item_id.clone();
            spawn_local({
                async move {
                    match api::delete_library(&id).await {
                        Ok(()) => {
                            libs.update(|l| l.retain(|lib| lib.item_id != id));
                            toast.set(Some("Library deleted".to_string()));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                    delete_target.set(None);
                }
            });
        }
    };

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                // Clear toast after display
                spawn_local({
                    async move {
                        gloo_net::http::Request::get("data:,")
                            .send().await.ok(); // tiny delay
                        toast.set(None);
                    }
                });
                view! {
                    div(class="toast-container") {
                        div(class="toast toast-success") { (msg) }
                    }
                }
            } else {
                view! {}
            })

            // Delete confirmation modal
            (if let Some(target) = delete_target.get_clone() {
                view! {
                    div(class="modal-overlay") {
                        div(class="modal") {
                            h3 { "Delete Library" }
                            p { "Are you sure you want to delete \"" (target.name) "\"? This cannot be undone." }
                            div(class="modal-actions") {
                                button(class="btn btn-secondary", on:click=move |_| delete_target.set(None)) { "Cancel" }
                                button(class="btn btn-danger", on:click=confirm_delete) { "Delete" }
                            }
                        }
                    }
                }
            } else {
                view! {}
            })

            div(class="page-header") {
                h1 { "Libraries" }
            }

            // Add Library form
            div(class="card mb-24") {
                h2 { "Add Library" }
                div(class="form-group") {
                    label { "Name" }
                    input(r#type="text", bind:value=name, placeholder="My Movies")
                }
                div(class="form-group") {
                    label { "Type" }
                    select(bind:value=collection_type) {
                        option(value="movies") { "Movies" }
                        option(value="tvshows") { "TV Shows" }
                        option(value="music") { "Music" }
                        option(value="mixed") { "Mixed" }
                    }
                }
                div(class="form-group") {
                    label { "Path" }
                    input(r#type="text", bind:value=path_input, placeholder="/media/movies")
                }
                button(class="btn btn-primary", on:click=on_create, disabled=creating.get()) {
                    (if creating.get() { "Creating..." } else { "Add Library" })
                }
            }

            // Error
            (if let Some(err) = error.get_clone() {
                view! { div(class="card") { p(class="text-muted") { (err) } } }
            } else {
                view! {}
            })

            // Libraries table
            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else if libs.get_clone().is_empty() {
                view! { div(class="empty-state") { "No libraries yet. Add one above." } }
            } else {
                let items = libs.get_clone();
                view! {
                    div(class="card") {
                        table {
                            thead {
                                tr {
                                    th { "Name" }
                                    th { "Type" }
                                    th { "Paths" }
                                    th { "Actions" }
                                }
                            }
                            tbody {
                                Indexed(
                                    list=move || items.clone(),
                                    view=move |lib| {
                                        let name = lib.name.clone();
                                        let ctype = lib.collection_type.clone().unwrap_or_else(|| "unknown".to_string());
                                        let locations = lib.locations.join(", ");
                                        let scan_id = lib.item_id.clone();
                                        let del_lib = lib.clone();
                                        view! {
                                            tr {
                                                td { (name) }
                                                td { span(class="badge badge-type") { (ctype) } }
                                                td { (locations) }
                                                td {
                                                    div(class="flex-gap") {
                                                        button(class="btn btn-secondary btn-sm",
                                                            on:click={
                                                                let scan_id = scan_id.clone();
                                                                move |_| on_scan(scan_id.clone())
                                                            }
                                                        ) { "Scan" }
                                                        button(class="btn btn-danger btn-sm",
                                                            on:click={
                                                                let del_lib = del_lib.clone();
                                                                move |_| delete_target.set(Some(del_lib.clone()))
                                                            }
                                                        ) { "Delete" }
                                                    }
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
