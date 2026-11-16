use sycamore::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::dashboard_view;
use crate::types::{
    CreateLibraryRequest, FileSystemEntry, UpdateLibraryRequest, VirtualFolderInfo,
};

#[component]
pub fn Libraries() -> View {
    let libs: Signal<Vec<VirtualFolderInfo>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);

    // Create form state
    let name = create_signal(String::new());
    let collection_type = create_signal("movies".to_string());
    let path_inputs: Signal<Vec<String>> = create_signal(vec![String::new()]);
    let creating = create_signal(false);

    // Folder picker state
    let browse_open = create_signal(false);
    let browse_path: Signal<String> = create_signal("/".to_string());
    let browse_entries: Signal<Vec<FileSystemEntry>> = create_signal(Vec::new());
    let browse_loading = create_signal(false);
    let browse_error: Signal<Option<String>> = create_signal(None);
    let browse_target_index: Signal<Option<usize>> = create_signal(None);

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

    // Edit modal state
    let edit_target: Signal<Option<VirtualFolderInfo>> = create_signal(None);
    let edit_name = create_signal(String::new());
    let edit_paths: Signal<Vec<String>> = create_signal(Vec::new());
    let edit_saving = create_signal(false);

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
        let paths: Vec<String> = path_inputs
            .get_clone()
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect();
        if n.is_empty() || paths.is_empty() {
            return;
        }
        creating.set(true);
        spawn_local({
            async move {
                let req = CreateLibraryRequest {
                    name: n,
                    collection_type: ct,
                    paths,
                };
                match api::create_library(&req).await {
                    Ok(lib) => {
                        libs.update(|l| l.push(lib));
                        name.set(String::new());
                        path_inputs.set(vec![String::new()]);
                        toast.set(Some("Library created".to_string()));
                    }
                    Err(e) => error.set(Some(e)),
                }
                creating.set(false);
            }
        });
    };

    let polling = create_signal(false);
    let on_scan = move |id: String| {
        // Show it at once; the server's RefreshStatus takes over on the next poll.
        libs.update(|list| {
            for lib in list.iter_mut().filter(|l| l.item_id == id) {
                lib.refresh_status = Some("Active".to_string());
            }
        });
        spawn_local({
            async move {
                match api::scan_library(Some(&id)).await {
                    Ok(()) => {
                        toast.set(Some("Scan started".to_string()));
                        if !polling.get() {
                            polling.set(true);
                            poll_scans(libs, toast, polling);
                        }
                    }
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

    let save_edit = move |_| {
        if let Some(target) = edit_target.get_clone() {
            let id = target.item_id.clone();
            let n = edit_name.get_clone();
            let paths: Vec<String> = edit_paths
                .get_clone()
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            if n.is_empty() || paths.is_empty() {
                return;
            }
            edit_saving.set(true);
            spawn_local({
                async move {
                    let req = UpdateLibraryRequest { name: n, paths };
                    match api::update_library(&id, &req).await {
                        Ok(updated_lib) => {
                            libs.update(|l| {
                                if let Some(lib) = l.iter_mut().find(|lib| lib.item_id == id) {
                                    lib.name = updated_lib.name;
                                    lib.locations = updated_lib.locations;
                                }
                            });
                            edit_target.set(None);
                            toast.set(Some("Library updated".to_string()));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                    edit_saving.set(false);
                }
            });
        }
    };

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                let msg_for_clear = msg.clone();
                // Clear toast after display
                spawn_local({
                    async move {
                        sleep_ms(3000).await;
                        if toast.get_clone().as_deref() == Some(msg_for_clear.as_str()) {
                            toast.set(None);
                        }
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

            // Edit modal
            (if edit_target.get_clone().is_some() {
                let current_edit_paths = edit_paths.get_clone();
                view! {
                    div(class="modal-overlay") {
                        div(class="modal") {
                            h3 { "Edit Library" }
                            div(class="form-group") {
                                label { "Name" }
                                input(r#type="text", bind:value=edit_name)
                            }
                            div(class="form-group") {
                                label { "Paths" }
                                div(class="path-list") {
                                    Indexed(
                                        list=move || current_edit_paths.clone(),
                                        view={
                                            let load_directory = load_directory;
                                            move |path| {
                                                let idx = edit_paths.get_clone().iter().position(|p| *p == path).unwrap_or(0);
                                                let path_val = path.clone();
                                                view! {
                                                    div(class="path-entry") {
                                                        input(r#type="text", value=path_val.clone(), on:input={
                                                            move |ev: web_sys::Event| {
                                                                let target = ev.target().unwrap();
                                                                let input: web_sys::HtmlInputElement = target.unchecked_into();
                                                                edit_paths.update(|paths| {
                                                                    if idx < paths.len() {
                                                                        paths[idx] = input.value();
                                                                    }
                                                                });
                                                            }
                                                        })
                                                        button(class="btn btn-secondary btn-sm", r#type="button", on:click={
                                                            let load_directory = load_directory;
                                                            move |_| {
                                                                browse_target_index.set(Some(idx));
                                                                browse_open.set(true);
                                                                let start = {
                                                                    let paths = edit_paths.get_clone();
                                                                    let current = paths.get(idx).cloned().unwrap_or_default();
                                                                    if current.is_empty() { "/".to_string() } else { current }
                                                                };
                                                                load_directory(start);
                                                            }
                                                        }) { "Browse" }
                                                        button(class="btn btn-danger btn-sm", r#type="button", on:click={
                                                            move |_| {
                                                                edit_paths.update(|paths| {
                                                                    if paths.len() > 1 && idx < paths.len() {
                                                                        paths.remove(idx);
                                                                    }
                                                                });
                                                            }
                                                        }) { "Remove" }
                                                    }
                                                }
                                            }
                                        }
                                    )
                                }
                                button(class="btn btn-secondary btn-sm", r#type="button", on:click=move |_| {
                                    edit_paths.update(|paths| paths.push(String::new()));
                                }) { "+ Add Path" }
                            }
                            div(class="modal-actions") {
                                button(class="btn btn-secondary", on:click=move |_| edit_target.set(None)) { "Cancel" }
                                button(class="btn btn-primary", on:click=save_edit, disabled=edit_saving.get()) {
                                    (if edit_saving.get() { "Saving..." } else { "Save" })
                                }
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
                                                move |entry| {
                                                    let entry_path = entry.path.clone();
                                                    let entry_name = entry.name.clone();
                                                    view! {
                                                        div(class="folder-item", on:click={
                                                            let entry_path = entry_path.clone();
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
                                    let selected_path = browse_path.get_clone();
                                    if let Some(idx) = browse_target_index.get_clone() {
                                        // Targeting an edit or create path at a specific index
                                        if edit_target.get_clone().is_some() {
                                            edit_paths.update(|paths| {
                                                if idx < paths.len() {
                                                    paths[idx] = selected_path;
                                                }
                                            });
                                        } else {
                                            path_inputs.update(|paths| {
                                                if idx < paths.len() {
                                                    paths[idx] = selected_path;
                                                }
                                            });
                                        }
                                    }
                                    browse_target_index.set(None);
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
                    label { "Paths" }
                    div(class="path-list") {
                        Indexed(
                            list=move || path_inputs.get_clone(),
                            view={
                                let load_directory = load_directory;
                                move |path| {
                                    let idx = path_inputs.get_clone().iter().position(|p| *p == path).unwrap_or(0);
                                    let path_val = path.clone();
                                    view! {
                                        div(class="path-entry") {
                                            input(r#type="text", value=path_val, placeholder="/media/movies", on:input={
                                                move |ev: web_sys::Event| {
                                                    let target = ev.target().unwrap();
                                                    let input: web_sys::HtmlInputElement = target.unchecked_into();
                                                    path_inputs.update(|paths| {
                                                        if idx < paths.len() {
                                                            paths[idx] = input.value();
                                                        }
                                                    });
                                                }
                                            })
                                            button(class="btn btn-secondary btn-sm", r#type="button", on:click={
                                                let load_directory = load_directory;
                                                move |_| {
                                                    browse_target_index.set(Some(idx));
                                                    browse_open.set(true);
                                                    let start = {
                                                        let paths = path_inputs.get_clone();
                                                        let current = paths.get(idx).cloned().unwrap_or_default();
                                                        if current.is_empty() { "/".to_string() } else { current }
                                                    };
                                                    load_directory(start);
                                                }
                                            }) { "Browse" }
                                            (if path_inputs.get_clone().len() > 1 {
                                                let idx = idx;
                                                view! {
                                                    button(class="btn btn-danger btn-sm", r#type="button", on:click=move |_| {
                                                        path_inputs.update(|paths| {
                                                            if idx < paths.len() {
                                                                paths.remove(idx);
                                                            }
                                                        });
                                                    }) { "Remove" }
                                                }
                                            } else {
                                                view! {}
                                            })
                                        }
                                    }
                                }
                            }
                        )
                    }
                    button(class="btn btn-secondary btn-sm", r#type="button", on:click=move |_| {
                        path_inputs.update(|paths| paths.push(String::new()));
                    }) { "+ Add Path" }
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
                                        let scanning = dashboard_view::is_scanning(lib.refresh_status.as_deref());
                                        let last = dashboard_view::last_scanned_label(lib.last_scanned.as_deref());
                                        let del_lib = lib.clone();
                                        let edit_lib = lib.clone();
                                        view! {
                                            tr {
                                                td {
                                                    div { (name) }
                                                    div(class="text-muted text-small") { (last) }
                                                }
                                                td { span(class="badge badge-type") { (ctype) } }
                                                td { (locations) }
                                                td {
                                                    div(class="flex-gap") {
                                                        button(class="btn btn-secondary btn-sm",
                                                            on:click={
                                                                let edit_lib = edit_lib.clone();
                                                                move |_| {
                                                                    edit_name.set(edit_lib.name.clone());
                                                                    edit_paths.set(edit_lib.locations.clone());
                                                                    edit_target.set(Some(edit_lib.clone()));
                                                                }
                                                            }
                                                        ) { "Edit" }
                                                        button(class="btn btn-secondary btn-sm",
                                                            disabled=scanning,
                                                            on:click={
                                                                let scan_id = scan_id.clone();
                                                                move |_| on_scan(scan_id.clone())
                                                            }
                                                        ) {
                                                            (if scanning {
                                                                view! { span(class="spinner spinner-sm") {} " Scanning…" }
                                                            } else {
                                                                view! { "Scan" }
                                                            })
                                                        }
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

/// Re-read the libraries every 2 seconds while any is being scanned, and say
/// when each scan finishes.
fn poll_scans(
    libs: Signal<Vec<VirtualFolderInfo>>,
    toast: Signal<Option<String>>,
    polling: Signal<bool>,
) {
    spawn_local(async move {
        loop {
            sleep_ms(2000).await;
            let Ok(now) = api::get_libraries().await else {
                polling.set(false);
                break;
            };
            let flags = |list: &[VirtualFolderInfo]| -> Vec<(String, bool)> {
                list.iter()
                    .map(|l| {
                        (
                            l.name.clone(),
                            dashboard_view::is_scanning(l.refresh_status.as_deref()),
                        )
                    })
                    .collect()
            };
            let before = flags(&libs.get_clone());
            let after = flags(&now);
            let done = dashboard_view::scans_finished(&before, &after);
            let any_active = after.iter().any(|(_, active)| *active);
            libs.set(now);
            if !done.is_empty() {
                toast.set(Some(format!("Scan finished: {}", done.join(", "))));
            }
            if !any_active {
                polling.set(false);
                break;
            }
        }
    });
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
