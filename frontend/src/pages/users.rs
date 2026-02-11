use sycamore::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::{UpdateUserPolicyRequest, UserDto, VirtualFolderInfo};

#[component]
pub fn Users() -> View {
    let users: Signal<Vec<UserDto>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);

    // Delete modal
    let delete_target: Signal<Option<UserDto>> = create_signal(None);

    // Library access modal state
    let lib_access_target: Signal<Option<UserDto>> = create_signal(None);
    let all_libraries: Signal<Vec<VirtualFolderInfo>> = create_signal(Vec::new());
    let lib_access_enable_all = create_signal(true);
    let lib_access_selected: Signal<Vec<String>> = create_signal(Vec::new());
    let lib_access_saving = create_signal(false);

    let load_users = move || {
        spawn_local({
            async move {
                match api::get_users().await {
                    Ok(u) => users.set(u),
                    Err(e) => error.set(Some(e)),
                }
                loading.set(false);
            }
        });
    };

    load_users();

    let on_toggle_admin = move |user_id: String, current_admin: bool| {
        // Read the current user's library settings to preserve them
        let user = users.get_clone().into_iter().find(|u| u.id == user_id);
        spawn_local({
            async move {
                let (enable_all, folders) = if let Some(u) = user {
                    (u.policy.enable_all_folders, u.policy.enabled_folders.clone())
                } else {
                    (true, Vec::new())
                };
                let req = UpdateUserPolicyRequest {
                    is_administrator: !current_admin,
                    enable_all_folders: enable_all,
                    enabled_folders: folders,
                };
                match api::update_user_policy(&user_id, &req).await {
                    Ok(()) => {
                        users.update(|list| {
                            for u in list.iter_mut() {
                                if u.id == user_id {
                                    u.policy.is_administrator = !current_admin;
                                }
                            }
                        });
                        toast.set(Some("User policy updated".to_string()));
                    }
                    Err(e) => error.set(Some(e)),
                }
            }
        });
    };

    let confirm_delete = move |_| {
        if let Some(target) = delete_target.get_clone() {
            let id = target.id.clone();
            spawn_local({
                async move {
                    match api::delete_user(&id).await {
                        Ok(()) => {
                            users.update(|list| list.retain(|u| u.id != id));
                            toast.set(Some("User deleted".to_string()));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                    delete_target.set(None);
                }
            });
        }
    };

    let open_lib_access = move |user: UserDto| {
        lib_access_enable_all.set(user.policy.enable_all_folders);
        lib_access_selected.set(user.policy.enabled_folders.clone());
        lib_access_target.set(Some(user));
        // Load libraries
        spawn_local({
            async move {
                match api::get_libraries().await {
                    Ok(libs) => all_libraries.set(libs),
                    Err(e) => error.set(Some(e)),
                }
            }
        });
    };

    let save_lib_access = move |_| {
        if let Some(target) = lib_access_target.get_clone() {
            let user_id = target.id.clone();
            let enable_all = lib_access_enable_all.get();
            let selected = lib_access_selected.get_clone();
            let is_admin = target.policy.is_administrator;
            lib_access_saving.set(true);
            spawn_local({
                async move {
                    let req = UpdateUserPolicyRequest {
                        is_administrator: is_admin,
                        enable_all_folders: enable_all,
                        enabled_folders: if enable_all {
                            Vec::new()
                        } else {
                            selected.clone()
                        },
                    };
                    match api::update_user_policy(&user_id, &req).await {
                        Ok(()) => {
                            users.update(|list| {
                                for u in list.iter_mut() {
                                    if u.id == user_id {
                                        u.policy.enable_all_folders = enable_all;
                                        u.policy.enabled_folders = if enable_all {
                                            Vec::new()
                                        } else {
                                            selected.clone()
                                        };
                                    }
                                }
                            });
                            lib_access_target.set(None);
                            toast.set(Some("Library access updated".to_string()));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                    lib_access_saving.set(false);
                }
            });
        }
    };

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                spawn_local({
                    async move {
                        gloo_net::http::Request::get("data:,")
                            .send().await.ok();
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
                            h3 { "Delete User" }
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

            // Library access modal
            (if let Some(target) = lib_access_target.get_clone() {
                let libs = all_libraries.get_clone();
                view! {
                    div(class="modal-overlay") {
                        div(class="modal") {
                            h3 { "Library Access — " (target.name) }
                            div(class="toggle-row") {
                                span { "Access all libraries" }
                                input(r#type="checkbox",
                                    checked=lib_access_enable_all.get(),
                                    on:change=move |ev: web_sys::Event| {
                                        let target = ev.target().unwrap();
                                        let input: web_sys::HtmlInputElement = target.unchecked_into();
                                        lib_access_enable_all.set(input.checked());
                                    }
                                )
                            }
                            (if !lib_access_enable_all.get() {
                                let libs = libs.clone();
                                view! {
                                    div(class="library-checklist") {
                                        Indexed(
                                            list=move || libs.clone(),
                                            view=move |lib| {
                                                let lib_id = lib.item_id.clone();
                                                let lib_name = lib.name.clone();
                                                let is_checked = lib_access_selected.get_clone().contains(&lib_id);
                                                view! {
                                                    label(class="library-check-item") {
                                                        input(r#type="checkbox",
                                                            checked=is_checked,
                                                            on:change={
                                                                let lib_id = lib_id.clone();
                                                                move |ev: web_sys::Event| {
                                                                    let target = ev.target().unwrap();
                                                                    let input: web_sys::HtmlInputElement = target.unchecked_into();
                                                                    lib_access_selected.update(|selected| {
                                                                        if input.checked() {
                                                                            if !selected.contains(&lib_id) {
                                                                                selected.push(lib_id.clone());
                                                                            }
                                                                        } else {
                                                                            selected.retain(|id| id != &lib_id);
                                                                        }
                                                                    });
                                                                }
                                                            }
                                                        )
                                                        span { (lib_name) }
                                                    }
                                                }
                                            }
                                        )
                                    }
                                }
                            } else {
                                view! {}
                            })
                            div(class="modal-actions") {
                                button(class="btn btn-secondary", on:click=move |_| lib_access_target.set(None)) { "Cancel" }
                                button(class="btn btn-primary", on:click=save_lib_access, disabled=lib_access_saving.get()) {
                                    (if lib_access_saving.get() { "Saving..." } else { "Save" })
                                }
                            }
                        }
                    }
                }
            } else {
                view! {}
            })

            div(class="page-header") {
                h1 { "Users" }
            }

            p(class="text-muted mb-24") {
                "Users are automatically provisioned when they first authenticate."
            }

            // Error
            (if let Some(err) = error.get_clone() {
                view! { div(class="card") { p(class="text-muted") { (err) } } }
            } else {
                view! {}
            })

            // Users table
            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else if users.get_clone().is_empty() {
                view! { div(class="empty-state") { "No users yet." } }
            } else {
                let items = users.get_clone();
                view! {
                    div(class="card") {
                        table {
                            thead {
                                tr {
                                    th { "Username" }
                                    th { "Role" }
                                    th { "Libraries" }
                                    th { "Last Activity" }
                                    th { "Actions" }
                                }
                            }
                            tbody {
                                Indexed(
                                    list=move || items.clone(),
                                    view=move |user| {
                                        let name = user.name.clone();
                                        let toggle_id = user.id.clone();
                                        let is_admin = user.policy.is_administrator;
                                        let last_activity = user.last_activity_date.clone().unwrap_or_else(|| "Never".to_string());
                                        let del_user = user.clone();
                                        let lib_user = user.clone();
                                        let lib_access_label = if user.policy.enable_all_folders {
                                            "All".to_string()
                                        } else {
                                            format!("{} selected", user.policy.enabled_folders.len())
                                        };
                                        view! {
                                            tr {
                                                td { (name) }
                                                td {
                                                    (if is_admin {
                                                        view! { span(class="badge badge-admin") { "Admin" } }
                                                    } else {
                                                        view! { span(class="text-muted") { "User" } }
                                                    })
                                                }
                                                td {
                                                    span(class="text-muted") { (lib_access_label) }
                                                }
                                                td(class="text-muted") {
                                                    (last_activity)
                                                }
                                                td {
                                                    div(class="flex-gap") {
                                                        button(class="btn btn-secondary btn-sm",
                                                            on:click={
                                                                let lib_user = lib_user.clone();
                                                                move |_| open_lib_access(lib_user.clone())
                                                            }
                                                        ) { "Libraries" }
                                                        button(class="btn btn-secondary btn-sm",
                                                            on:click={
                                                                let toggle_id = toggle_id.clone();
                                                                move |_| on_toggle_admin(toggle_id.clone(), is_admin)
                                                            }
                                                        ) {
                                                            (if is_admin { "Revoke Admin" } else { "Make Admin" })
                                                        }
                                                        button(class="btn btn-danger btn-sm",
                                                            on:click={
                                                                let del_user = del_user.clone();
                                                                move |_| delete_target.set(Some(del_user.clone()))
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
