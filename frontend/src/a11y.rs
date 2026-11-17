//! Accessibility rules the whole admin shares (roadmap 12f): page titles that
//! screen readers announce, and arrow-key movement in lists.

/// The page's name for the browser tab and screen-reader announcement.
pub fn page_title(path: &str) -> &'static str {
    let _ = path;
    ""
}

/// Where focus goes in a list of `len` items from `current` on `key`
/// (ArrowDown/ArrowUp move one, Home/End jump; other keys do nothing).
pub fn next_index(current: Option<usize>, len: usize, key: &str) -> Option<usize> {
    let _ = (current, len, key);
    None
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
