use sycamore::prelude::*;
use sycamore_router::navigate;

fn nav_click(e: web_sys::MouseEvent, path: &str) {
    e.prevent_default();
    e.stop_propagation();
    navigate(path);
}

#[component]
pub fn Sidebar() -> View {
    view! {
        nav(class="sidebar") {
            div(class="sidebar-brand") { "Rustyfin" }
            div(class="sidebar-nav") {
                a(href="/web", on:click=|e: web_sys::MouseEvent| nav_click(e, "/web")) { "Dashboard" }
                a(href="/web/libraries", on:click=|e: web_sys::MouseEvent| nav_click(e, "/web/libraries")) { "Libraries" }
                a(href="/web/media", on:click=|e: web_sys::MouseEvent| nav_click(e, "/web/media")) { "Media" }
                a(href="/web/users", on:click=|e: web_sys::MouseEvent| nav_click(e, "/web/users")) { "Users" }
                a(href="/web/tasks", on:click=|e: web_sys::MouseEvent| nav_click(e, "/web/tasks")) { "Tasks" }
            }
        }
    }
}
