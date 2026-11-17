//! `rustyfin artwork -t music`: album covers from the Cover Art Archive for
//! albums with no cover file (owner, 2026-11-16: new Lidarr albums had no
//! picture anywhere). Albums are found by the MusicBrainz ids Lidarr and
//! Picard already write (`album.nfo`, else the first track's tags), never by
//! name. The cover is saved as `cover.jpg` in the album folder, which the
//! server picks up at its next scan. Existing covers are never replaced
//! (`--force` does). Albums the archive has no cover for record
//! `[artwork] missing_checked` in their `.rustyfin.toml` and are not asked
//! again for 30 days.

use std::path::{Path, PathBuf};

pub fn album_dirs(_root: &Path) -> Vec<PathBuf> {
    Vec::new()
}

pub fn has_cover(_dir: &Path) -> bool {
    false
}

pub fn album_mbids(_dir: &Path) -> (Option<String>, Option<String>) {
    (None, None)
}

pub fn cover_urls(_base: &str, _release: Option<&str>, _group: Option<&str>) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "rustyfin-albumart-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn albums_are_the_folders_that_hold_tracks() {
        let t = Tree::new();
        t.file("Wavves/Cherry Soda (2026)/01 We Get Around.flac", "");
        t.file("Wavves/Cherry Soda (2026)/02 Pinesol.flac", "");
        t.file("Wavves/artist.nfo", "<artist/>");
        t.file("Jeff Beck/Live in Tokyo (2014)/CD1/01 Loaded.mp3", "");
        let dirs = album_dirs(&t.0);
        assert_eq!(
            dirs,
            [
                t.0.join("Jeff Beck/Live in Tokyo (2014)/CD1"),
                t.0.join("Wavves/Cherry Soda (2026)"),
            ]
        );
    }

    #[test]
    fn a_folder_with_a_cover_file_needs_none() {
        let t = Tree::new();
        t.file("A/B/01.flac", "");
        assert!(!has_cover(&t.0.join("A/B")));
        t.file("A/B/folder.jpg", "x");
        assert!(has_cover(&t.0.join("A/B")));
    }

    #[test]
    fn lidarrs_album_nfo_names_the_release() {
        let t = Tree::new();
        t.file("A/B/01.flac", "");
        t.file(
            "A/B/album.nfo",
            "<album><title>Cherry Soda</title>\
             <musicbrainzalbumid>d56aacad-e477-4921-b858-40bda0d8b5e3</musicbrainzalbumid>\
             <musicbrainzreleasegroupid>28b5d5e5-3c90-41c6-a736-21effb7d9e5d</musicbrainzreleasegroupid>\
             </album>",
        );
        assert_eq!(
            album_mbids(&t.0.join("A/B")),
            (
                Some("d56aacad-e477-4921-b858-40bda0d8b5e3".to_string()),
                Some("28b5d5e5-3c90-41c6-a736-21effb7d9e5d".to_string())
            )
        );
        t.file("C/D/01.flac", "");
        assert_eq!(album_mbids(&t.0.join("C/D")), (None, None));
    }

    #[test]
    fn the_release_is_tried_before_its_group() {
        assert_eq!(
            cover_urls(
                "https://coverartarchive.org/",
                Some("d56aacad-e477"),
                Some("28b5d5e5-3c90")
            ),
            [
                "https://coverartarchive.org/release/d56aacad-e477/front-1200",
                "https://coverartarchive.org/release-group/28b5d5e5-3c90/front-1200"
            ]
        );
        assert_eq!(
            cover_urls("https://c", None, Some("ab-12")),
            ["https://c/release-group/ab-12/front-1200"]
        );
        // Anything that is not an id is never put in a URL.
        assert!(cover_urls("https://c", Some("../x?y"), None).is_empty());
        assert!(cover_urls("https://c", None, None).is_empty());
    }
}
