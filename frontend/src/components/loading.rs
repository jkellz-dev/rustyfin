//! One loading placeholder for every page (roadmap 12g): grey shapes where
//! the content will be, instead of a spinner and "Loading...".

use sycamore::prelude::*;

/// `kind`: "grid" (poster cards), "detail" (a title page), "rows" (a list).
#[component(inline_props)]
pub fn Loading(kind: &'static str) -> View {
    let (class, count) = match kind {
        "grid" => ("skeleton-grid", 12),
        "detail" => ("skeleton-detail", 1),
        _ => ("skeleton-rows", 6),
    };
    view! {
        div(class=class, aria-busy="true", aria-label="Loading") {
            Indexed(
                list=move || (0..count).collect::<Vec<i32>>(),
                view=|_| view! { div(class="skeleton-block") }
            )
        }
    }
}
