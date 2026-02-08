use sycamore::prelude::*;

#[component(inline_props)]
pub fn Layout(children: Children) -> View {
    let children = children.call();
    view! {
        div(class="app-layout") {
            nav(class="sidebar") {
                div(class="sidebar-brand") { "Rustyfin" }
                div(class="sidebar-nav") {
                    a(href="/web") { "Dashboard" }
                    a(href="/web/libraries") { "Libraries" }
                    a(href="/web/users") { "Users" }
                }
            }
            main(class="main-content") {
                (children)
            }
        }
    }
}
