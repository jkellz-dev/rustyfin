//! Accessibility rules the whole admin shares (roadmap 12f): page titles that
//! screen readers announce, and arrow-key movement in lists.

/// The page's name for the browser tab and screen-reader announcement.
pub fn page_title(path: &str) -> &'static str {
    let rest = path.strip_prefix("/web").unwrap_or("").trim_matches('/');
    let mut parts = rest.split('/');
    match (parts.next(), parts.next()) {
        (Some("libraries"), _) => "Libraries",
        (Some("media"), _) => "Media",
        (Some("users"), None) => "Users",
        (Some("users"), Some(_)) => "User",
        (Some("tasks"), _) => "Scheduled tasks",
        (Some("setup"), _) => "Setup",
        _ => "Dashboard",
    }
}

/// Where focus goes in a list of `len` items from `current` on `key`
/// (ArrowDown/ArrowUp move one, Home/End jump; other keys do nothing).
pub fn next_index(current: Option<usize>, len: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    match (key, current) {
        ("ArrowDown", None) | ("Home", _) => Some(0),
        ("ArrowDown", Some(i)) => Some((i + 1).min(last)),
        ("ArrowUp", None) | ("End", _) => Some(last),
        ("ArrowUp", Some(i)) => Some(i.saturating_sub(1)),
        _ => None,
    }
}

// ── In the browser ─────────────────────────────────────────

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

thread_local! {
    /// What had focus before a dialog opened, to give it back on close.
    static BEFORE_DIALOG: std::cell::RefCell<Option<web_sys::HtmlElement>> =
        const { std::cell::RefCell::new(None) };
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// Name the page in the tab and say it to screen readers.
pub fn announce_page(path: &str) {
    let title = page_title(path);
    // After the page is drawn: on the first draw the live region is not in
    // the document yet.
    wasm_bindgen_futures::spawn_local(async move {
        say_page(title);
    });
}

fn say_page(title: &str) {
    if let Some(doc) = document() {
        doc.set_title(&format!("{title} · Rustyfin Admin"));
        if let Some(region) = doc.get_element_by_id("sr-announcer") {
            region.set_text_content(Some(&format!("{title} page")));
        }
    }
}

/// Keyboard and focus behaviour for the whole admin, installed once:
/// Escape closes a dialog (its Cancel button), a dialog gets focus when it
/// opens and gives it back when it closes, and arrow keys move through
/// lists marked `data-arrow-list` (the folder picker).
pub fn install() {
    let Some(doc) = document() else { return };
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let Some(doc) = document() else { return };
            let key = e.key();
            if key == "Escape" {
                if let Ok(Some(cancel)) =
                    doc.query_selector(".modal-overlay .modal-actions .btn-secondary")
                    && let Ok(cancel) = cancel.dyn_into::<web_sys::HtmlElement>()
                {
                    e.prevent_default();
                    cancel.click();
                }
                return;
            }
            let Some(active) = doc.active_element() else {
                return;
            };
            let Ok(Some(list)) = active.closest("[data-arrow-list]") else {
                return;
            };
            let Ok(items) = list.query_selector_all("button, a[href]") else {
                return;
            };
            let len = items.length() as usize;
            let current = (0..len).find(|&i| {
                items
                    .item(i as u32)
                    .is_some_and(|n| n.is_same_node(Some(active.unchecked_ref())))
            });
            if let Some(next) = next_index(current, len, &key)
                && let Some(node) = items.item(next as u32)
                && let Ok(el) = node.dyn_into::<web_sys::HtmlElement>()
            {
                e.prevent_default();
                let _ = el.focus();
            }
        });
    let _ = doc.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    on_key.forget();

    let on_change = Closure::<dyn FnMut(js_sys::Array, web_sys::MutationObserver)>::new(
        move |_: js_sys::Array, _: web_sys::MutationObserver| {
            let Some(doc) = document() else { return };
            let dialog = doc.query_selector(".modal").ok().flatten();
            BEFORE_DIALOG.with(|before| {
                let mut before = before.borrow_mut();
                match (&dialog, before.is_some()) {
                    (Some(d), false) => {
                        *before = doc
                            .active_element()
                            .and_then(|a| a.dyn_into::<web_sys::HtmlElement>().ok());
                        if let Ok(Some(first)) = d.query_selector("input, select, textarea, button")
                            && let Ok(first) = first.dyn_into::<web_sys::HtmlElement>()
                        {
                            let _ = first.focus();
                        }
                    }
                    (None, true) => {
                        if let Some(el) = before.take()
                            && el.is_connected()
                        {
                            let _ = el.focus();
                        }
                    }
                    _ => {}
                }
            });
        },
    );
    if let Ok(observer) = web_sys::MutationObserver::new(on_change.as_ref().unchecked_ref())
        && let Some(body) = doc.body()
    {
        let init = web_sys::MutationObserverInit::new();
        init.set_child_list(true);
        init.set_subtree(true);
        let _ = observer.observe_with_options(&body, &init);
    }
    on_change.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_a_name() {
        assert_eq!(page_title("/web"), "Dashboard");
        assert_eq!(page_title("/web/"), "Dashboard");
        assert_eq!(page_title("/web/libraries"), "Libraries");
        assert_eq!(page_title("/web/media"), "Media");
        assert_eq!(page_title("/web/media/item/abc"), "Media");
        assert_eq!(page_title("/web/users"), "Users");
        assert_eq!(page_title("/web/users/abc"), "User");
        assert_eq!(page_title("/web/tasks"), "Scheduled tasks");
        assert_eq!(page_title("/web/setup"), "Setup");
        assert_eq!(page_title("/elsewhere"), "Dashboard");
    }

    #[test]
    fn arrows_move_through_a_list() {
        assert_eq!(next_index(None, 3, "ArrowDown"), Some(0));
        assert_eq!(next_index(Some(0), 3, "ArrowDown"), Some(1));
        assert_eq!(
            next_index(Some(2), 3, "ArrowDown"),
            Some(2),
            "stays at the end"
        );
        assert_eq!(next_index(Some(1), 3, "ArrowUp"), Some(0));
        assert_eq!(next_index(Some(0), 3, "ArrowUp"), Some(0));
        assert_eq!(next_index(None, 3, "ArrowUp"), Some(2));
        assert_eq!(next_index(Some(1), 3, "Home"), Some(0));
        assert_eq!(next_index(Some(1), 3, "End"), Some(2));
        assert_eq!(next_index(Some(1), 3, "a"), None);
        assert_eq!(next_index(None, 0, "ArrowDown"), None, "an empty list");
    }
}
