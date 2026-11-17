//! A failed request, said plainly, with Retry and Dismiss (roadmap 12c). The
//! rest of the page stays: one failed call never blanks it.

use sycamore::prelude::*;

/// Shows `error` when set; Retry reloads the page's data (the page itself),
/// Dismiss hides the message.
#[component(inline_props)]
pub fn ErrorNotice(error: Signal<Option<String>>) -> View {
    view! {
        (if let Some(message) = error.get_clone() {
            view! {
                div(class="card mb-16 error-notice", role="alert") {
                    p { (message) }
                    div(class="flex-gap") {
                        button(class="btn btn-secondary btn-sm", on:click=move |_| {
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().reload();
                            }
                        }) { "Retry" }
                        button(class="btn btn-secondary btn-sm", on:click=move |_| error.set(None)) { "Dismiss" }
                    }
                }
            }
        } else {
            view! {}
        })
    }
}
