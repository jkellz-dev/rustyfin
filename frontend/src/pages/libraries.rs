use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::{CreateLibraryRequest, FileSystemEntry, VirtualFolderInfo};

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

    // Folder picker state
    let browse_open = create_signal(false);
    let browse_path: Signal<String> = create_signal("/".to_string());
    let browse_entries: Signal<Vec<FileSystemEntry>> = create_signal(Vec::new());
    let browse_loading = create_signal(false);
    let browse_error: Signal<Option<String>> = create_signal(None);

    let load_directory = move |dir: String| {
        browse_loading.set(true);
        browse_error.set(None);
        browse_path.set(dir.clone());
        spawn_local(async move {
            match api::get_directory_contents(&dir).await {
                Ok(entries) => browse_entries.set(entries),
                Err(e) => browse_error.set(Some(e)),
            }
            browse_loading.set(false);
        });
    };

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

            // Folder picker modal
            (if browse_open.get() {
                let current_path = browse_path.get_clone();
                let parent = {
                    let p = std::path::Path::new(&current_path);
                    p.parent().map(|pp| pp.to_string_lossy().into_owned()).unwrap_or_else(|| "/".to_string())
                };
                view! {
                    div(class="modal-overlay") {
                        div(class="modal folder-picker") {
                            h3 { "Browse Folders" }
                            div(class="folder-path") { (browse_path.get_clone()) }

                            div(class="folder-actions-top") {
                                button(class="btn btn-secondary btn-sm", on:click={
                                    let load_directory = load_directory.clone();
                                    let parent = parent.clone();
                                    move |_| load_directory(parent.clone())
                                }) { "Up" }
                            }

                            (if browse_loading.get() {
                                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
                            } else if let Some(err) = browse_error.get_clone() {
                                view! { div(class="folder-error") { (err) } }
                            } else if browse_entries.get_clone().is_empty() {
                                view! { div(class="folder-empty") { "No subdirectories" } }
                            } else {
                                let items = browse_entries.get_clone();
                                view! {
                                    div(class="folder-list") {
                                        Indexed(
                                            list=move || items.clone(),
                                            view={
                                                let load_directory = load_directory.clone();
                                                move |entry| {
                                                    let entry_path = entry.path.clone();
                                                    let entry_name = entry.name.clone();
                                                    let load_directory = load_directory.clone();
                                                    view! {
                                                        div(class="folder-item", on:click={
                                                            let entry_path = entry_path.clone();
                                                            let load_directory = load_directory.clone();
                                                            move |_| load_directory(entry_path.clone())
                                                        }) {
                                                            span(class="folder-icon") { "\u{1F4C1}" }
                                                            span { (entry_name) }
                                                        }
                                                    }
                                                }
                                            }
                                        )
                                    }
                                }
                            })

                            div(class="modal-actions") {
                                button(class="btn btn-secondary", on:click=move |_| browse_open.set(false)) { "Cancel" }
                                button(class="btn btn-primary", on:click=move |_| {
                                    path_input.set(browse_path.get_clone());
                                    browse_open.set(false);
                                }) { "Select" }
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
                    div(class="input-with-button") {
                        input(r#type="text", bind:value=path_input, placeholder="/media/movies")
                        button(class="btn btn-secondary", r#type="button", on:click={
                            let load_directory = load_directory.clone();
                            move |_| {
                                browse_open.set(true);
                                let start = {
                                    let current = path_input.get_clone();
                                    if current.is_empty() { "/".to_string() } else { current }
                                };
                                load_directory(start);
                            }
                        }) { "Browse" }
                    }
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
