use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::types::TaskInfoDto;

#[component]
pub fn Tasks() -> View {
    let tasks: Signal<Vec<TaskInfoDto>> = create_signal(Vec::new());
    let loading = create_signal(true);
    let error: Signal<Option<String>> = create_signal(None);
    let toast: Signal<Option<String>> = create_signal(None);
    let polling = create_signal(false);

    let load_tasks = move || {
        spawn_local(async move {
            match api::get_scheduled_tasks().await {
                Ok(t) => {
                    let any_running = t.iter().any(|task| task.state == "Running");
                    tasks.set(t);
                    // Start polling if any task is running
                    if any_running && !polling.get() {
                        polling.set(true);
                        start_polling(tasks, polling);
                    }
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    load_tasks();

    let on_run_task = move |task_id: String, task_name: String| {
        spawn_local(async move {
            match api::run_scheduled_task(&task_id).await {
                Ok(()) => {
                    toast.set(Some(format!("Started: {task_name}")));
                    if let Ok(t) = api::get_scheduled_tasks().await {
                        tasks.set(t);
                    }
                    if !polling.get() {
                        polling.set(true);
                        start_polling(tasks, polling);
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let on_stop_task = move |task_id: String, task_name: String| {
        spawn_local(async move {
            match api::stop_scheduled_task(&task_id).await {
                Ok(()) => {
                    toast.set(Some(format!("Stopped: {task_name}")));
                    if let Ok(t) = api::get_scheduled_tasks().await {
                        tasks.set(t);
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        div {
            // Toast
            (if let Some(msg) = toast.get_clone() {
                spawn_local(async move {
                    gloo_net::http::Request::get("data:,")
                        .send().await.ok();
                    toast.set(None);
                });
                view! {
                    div(class="toast-container") {
                        div(class="toast toast-success") { (msg) }
                    }
                }
            } else {
                view! {}
            })

            div(class="page-header") {
                h1 { "Scheduled Tasks" }
            }

            // Error
            (if let Some(err) = error.get_clone() {
                view! { div(class="card") { p(class="text-muted") { (err) } } }
            } else {
                view! {}
            })

            // Tasks table
            (if loading.get() {
                view! { div(class="loading-container") { span(class="spinner") {} " Loading..." } }
            } else {
                view! {
                    div(class="card") {
                        table {
                            thead {
                                tr {
                                    th { "Name" }
                                    th { "Category" }
                                    th { "State" }
                                    th { "Last Run" }
                                    th { "Result" }
                                    th { "Actions" }
                                }
                            }
                            tbody {
                                Indexed(
                                    list=move || tasks.get_clone(),
                                    view=move |task| {
                                        let name = task.name.clone();
                                        let category = task.category.clone();
                                        let state = task.state.clone();
                                        let run_id = task.id.clone();
                                        let stop_id = task.id.clone();
                                        let run_name = task.name.clone();
                                        let stop_name = task.name.clone();
                                        let is_running = state == "Running";

                                        let (last_run, result_status, result_error) =
                                            if let Some(ref r) = task.last_execution_result {
                                                (
                                                    format_datetime(&r.start_time_utc),
                                                    r.status.clone(),
                                                    r.error_message.clone(),
                                                )
                                            } else {
                                                ("Never".to_string(), String::new(), None)
                                            };

                                        view! {
                                            tr {
                                                td {
                                                    span { (name) }
                                                    (if !task.description.is_empty() {
                                                        let desc = task.description.clone();
                                                        view! { br {} span(class="text-muted text-sm") { (desc) } }
                                                    } else {
                                                        view! {}
                                                    })
                                                }
                                                td(class="text-muted") { (category) }
                                                td {
                                                    (if is_running {
                                                        view! { span(class="badge badge-running") { "Running" } }
                                                    } else {
                                                        view! { span(class="badge badge-idle") { "Idle" } }
                                                    })
                                                }
                                                td(class="text-muted") { (last_run) }
                                                td {
                                                    (match result_status.as_str() {
                                                        "Completed" => view! { span(class="badge badge-success") { "Completed" } },
                                                        "Failed" | "Error" => {
                                                            let err_msg = result_error.clone().unwrap_or_default();
                                                            view! {
                                                                span(class="badge badge-error", title=err_msg) { "Failed" }
                                                            }
                                                        }
                                                        "Cancelled" => view! { span(class="badge badge-warning") { "Cancelled" } },
                                                        "" => view! { span(class="text-muted") { "\u{2014}" } },
                                                        other => {
                                                            let s = other.to_string();
                                                            view! { span(class="text-muted") { (s) } }
                                                        }
                                                    })
                                                }
                                                td {
                                                    (if is_running {
                                                        view! {
                                                            button(
                                                                class="btn btn-danger btn-sm",
                                                                on:click={
                                                                    let stop_id = stop_id.clone();
                                                                    let stop_name = stop_name.clone();
                                                                    move |_| on_stop_task(stop_id.clone(), stop_name.clone())
                                                                }
                                                            ) { "Stop" }
                                                        }
                                                    } else {
                                                        view! {
                                                            button(
                                                                class="btn btn-primary btn-sm",
                                                                on:click={
                                                                    let run_id = run_id.clone();
                                                                    let run_name = run_name.clone();
                                                                    move |_| on_run_task(run_id.clone(), run_name.clone())
                                                                }
                                                            ) { "Run Now" }
                                                        }
                                                    })
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

/// Poll task state every 2 seconds until no tasks are running.
fn start_polling(tasks: Signal<Vec<TaskInfoDto>>, polling: Signal<bool>) {
    spawn_local(async move {
        loop {
            sleep_ms(2000).await;

            match api::get_scheduled_tasks().await {
                Ok(t) => {
                    let any_running = t.iter().any(|task| task.state == "Running");
                    tasks.set(t);
                    if !any_running {
                        polling.set(false);
                        break;
                    }
                }
                Err(_) => {
                    polling.set(false);
                    break;
                }
            }
        }
    });
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

/// Format an RFC 3339 datetime string to a shorter display form.
fn format_datetime(rfc3339: &str) -> String {
    // Extract "YYYY-MM-DD HH:MM" from "2026-02-20T14:30:00+00:00"
    if rfc3339.len() >= 16 {
        rfc3339[..16].replace('T', " ")
    } else {
        rfc3339.to_string()
    }
}
