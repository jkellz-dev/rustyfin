use sycamore::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::components::error_notice::ErrorNotice;
use crate::components::loading::Loading;
use crate::dashboard_view;
use crate::timer::sleep_ms;
use crate::types::TaskInfoDto;

#[component]
pub fn Tasks() -> View {
    api::new_page();
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
                Err(e) => error.set(Some(e.to_string())),
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
                Err(e) => error.set(Some(e.to_string())),
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
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    view! {
        div {
            // Toast
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
                h1 { "Scheduled Tasks" }
            }

            // Error
            ErrorNotice(error=error)

            // Tasks table
            (if loading.get() {
                view! { Loading(kind="rows") }
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
                                    th { "Took" }
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
                                        // How long the last run took (owner, 2026-11-20).
                                        let took = task.last_execution_result.as_ref().map_or_else(
                                            || dashboard_view::run_took(None, None, is_running),
                                            |r| {
                                                dashboard_view::run_took(
                                                    epoch_ms(&r.start_time_utc),
                                                    r.end_time_utc.as_deref().and_then(epoch_ms),
                                                    is_running,
                                                )
                                            },
                                        );

                                        view! {
                                            tr {
                                                td(data-label="Name") {
                                                    span { (name) }
                                                    (if !task.description.is_empty() {
                                                        let desc = task.description.clone();
                                                        view! { br {} span(class="text-muted text-sm") { (desc) } }
                                                    } else {
                                                        view! {}
                                                    })
                                                }
                                                td(class="text-muted", data-label="Category") { (category) }
                                                td(data-label="State") {
                                                    (if is_running {
                                                        view! { span(class="badge badge-running") { "Running" } }
                                                    } else {
                                                        view! { span(class="badge badge-idle") { "Idle" } }
                                                    })
                                                }
                                                td(class="text-muted", data-label="Last run") { (last_run) }
                                                td(class="text-muted", data-label="Took") { (took) }
                                                td(data-label="Result") {
                                                    (if result_status.is_empty() {
                                                        view! { span(class="text-muted") { "\u{2014}" } }
                                                    } else {
                                                        let (label, class, reason) = dashboard_view::run_result(
                                                            &result_status,
                                                            result_error.as_deref(),
                                                        );
                                                        let badge = format!("badge {class}");
                                                        view! {
                                                            span(class=badge) { (label) }
                                                            (match reason {
                                                                Some(why) => view! { br {} span(class="text-muted text-sm") { (why) } },
                                                                None => view! {},
                                                            })
                                                        }
                                                    })
                                                }
                                                td(data-label="Actions") {
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

/// Format an RFC 3339 datetime string to a shorter display form.
/// An RFC 3339 time in epoch milliseconds, if the browser can read it.
fn epoch_ms(rfc3339: &str) -> Option<f64> {
    let ms = js_sys::Date::parse(rfc3339);
    (!ms.is_nan()).then_some(ms)
}

fn format_datetime(rfc3339: &str) -> String {
    // Extract "YYYY-MM-DD HH:MM" from "2026-02-20T14:30:00+00:00"
    if rfc3339.len() >= 16 {
        rfc3339[..16].replace('T', " ")
    } else {
        rfc3339.to_string()
    }
}
