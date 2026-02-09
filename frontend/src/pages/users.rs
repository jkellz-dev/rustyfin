use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::{UpdateUserPolicyRequest, UserDto};

#[component]
pub fn Users() -> View {
    let users: Signal<Vec<UserDto>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);

    // Delete modal
    let delete_target: Signal<Option<UserDto>> = create_signal(None);

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
        spawn_local({
            async move {
                let req = UpdateUserPolicyRequest {
                    is_administrator: !current_admin,
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
                                                td(class="text-muted") {
                                                    (last_activity)
                                                }
                                                td {
                                                    div(class="flex-gap") {
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
