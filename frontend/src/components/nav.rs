use sycamore::prelude::*;
use sycamore_router::navigate;

fn nav_click(e: web_sys::MouseEvent, path: &str) {
    e.prevent_default();
    e.stop_propagation();
    navigate(path);
}

/// The admin's pages, in menu order.
const PAGES: [(&str, &str); 6] = [
    ("Dashboard", "/web"),
    ("Libraries", "/web/libraries"),
    ("Media", "/web/media"),
    ("Users", "/web/users"),
    ("Tasks", "/web/tasks"),
    ("Links", "/web/links"),
];

/// The class of the menu: `sidebar`, plus `open` while the phone menu is
/// shown (12h, owner 2026-11-16: on a phone the sidebar covered the page).
pub fn sidebar_class(open: bool) -> &'static str {
    if open { "sidebar open" } else { "sidebar" }
}

/// The side menu. On a wide screen it is a fixed column; on a phone (see
/// `style.css`, `max-width: 768px`) it is a bar across the top with a menu
/// button, and the links show only while it is open. Following a link
/// closes it.
#[component]
pub fn Sidebar() -> View {
    let open = create_signal(false);
    let links: Vec<View> = PAGES
        .iter()
        .map(|&(label, path)| {
            view! {
                a(href=path, on:click=move |e: web_sys::MouseEvent| {
                    open.set(false);
                    nav_click(e, path);
                }) { (label) }
            }
        })
        .collect();
    view! {
        nav(class=move || sidebar_class(open.get()), aria-label="Admin") {
            div(class="sidebar-brand") {
                span { "Rustyfin" }
                button(
                    class="nav-toggle",
                    r#type="button",
                    aria-label="Menu",
                    aria-expanded=move || open.get().to_string(),
                    on:click=move |_| open.set(!open.get()),
                ) {
                    span(class="nav-toggle-bar") {}
                    span(class="nav-toggle-bar") {}
                    span(class="nav-toggle-bar") {}
                }
            }
            div(class="sidebar-nav") { (links) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phone_menu_is_closed_until_opened() {
        assert_eq!(sidebar_class(false), "sidebar");
        assert_eq!(sidebar_class(true), "sidebar open");
    }

    #[test]
    fn every_page_is_in_the_menu() {
        let paths: Vec<&str> = PAGES.iter().map(|p| p.1).collect();
        assert_eq!(
            paths,
            [
                "/web",
                "/web/libraries",
                "/web/media",
                "/web/users",
                "/web/tasks",
                "/web/links"
            ]
        );
    }
}
