use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::CreateLibraryRequest;

#[component]
pub fn Setup() -> View {
    let step = create_signal(0u32); // 0=Welcome, 1=Add Library, 2=Done
    let error: Signal<Option<String>> = create_signal(None);
    let checking = create_signal(true);

    // Form state for step 1
    let name = create_signal(String::new());
    let collection_type = create_signal("movies".to_string());
    let path_input = create_signal(String::new());
    let creating = create_signal(false);

    // Check if wizard already complete
    spawn_local({
        async move {
            if let Ok(config) = api::get_startup_config().await
                && config.startup_wizard_completed
            {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/web");
                }
                return;
            }
            checking.set(false);
        }
    });

    let on_next = move |_| {
        step.set(step.get() + 1);
    };

    let on_create_and_scan = move |_| {
        let n = name.get_clone();
        let ct = collection_type.get_clone();
        let p = path_input.get_clone();
        if n.is_empty() || p.is_empty() {
            error.set(Some("Name and path are required".to_string()));
            return;
        }
        creating.set(true);
        error.set(None);
        spawn_local({
            async move {
                let req = CreateLibraryRequest {
                    name: n,
                    collection_type: ct,
                    paths: vec![p],
                };
                match api::create_library(&req).await {
                    Ok(lib) => {
                        // Auto-trigger scan
                        let _ = api::scan_library(Some(&lib.item_id)).await;
                        step.set(2);
                    }
                    Err(e) => error.set(Some(e)),
                }
                creating.set(false);
            }
        });
    };

    let on_finish = move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/web");
        }
    };

    view! {
        div(class="setup-layout") {
            (if checking.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Checking..." } }
            } else {
                view! {
                    div(class="setup-container") {
                        // Step indicators
                        div(class="steps") {
                            div(class="step active") {}
                            div(class=if step.get() >= 1 { "step active" } else { "step" }) {}
                            div(class=if step.get() >= 2 { "step active" } else { "step" }) {}
                        }

                        (match step.get() {
                            0 => view! {
                                h1 { "Welcome to Rustyfin" }
                                p { "Let's set up your media server. You'll add your first library to get started." }
                                button(class="btn btn-primary", on:click=on_next) { "Get Started" }
                            },
                            1 => view! {
                                h1 { "Add Your First Library" }
                                p { "Point Rustyfin to a folder containing your media files." }

                                (if let Some(err) = error.get_clone() {
                                    view! { p(class="text-muted") { (err) } }
                                } else {
                                    view! {}
                                })

                                div(class="form-group") {
                                    label { "Library Name" }
                                    input(r#type="text", bind:value=name, placeholder="Movies")
                                }
                                div(class="form-group") {
                                    label { "Content Type" }
                                    select(bind:value=collection_type) {
                                        option(value="movies") { "Movies" }
                                        option(value="tvshows") { "TV Shows" }
                                        option(value="music") { "Music" }
                                        option(value="mixed") { "Mixed" }
                                    }
                                }
                                div(class="form-group") {
                                    label { "Media Folder Path" }
                                    input(r#type="text", bind:value=path_input, placeholder="/media/movies")
                                }
                                button(class="btn btn-primary", on:click=on_create_and_scan, disabled=creating.get()) {
                                    (if creating.get() { "Creating..." } else { "Add Library & Scan" })
                                }
                            },
                            _ => view! {
                                h1 { "You're All Set!" }
                                p { "Your library has been created and a scan has been started. Media will appear as it's discovered." }
                                button(class="btn btn-primary", on:click=on_finish) { "Go to Dashboard" }
                            },
                        })
                    }
                }
            })
        }
    }
}
