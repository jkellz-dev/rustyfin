//! Owner, 2026-11-24: a music scan that changed almost nothing took 44
//! minutes, "updated=65993, skipped=1522": every track's change tag held its
//! album NFO's modification time, and the NFO writer rewrites album.nfo on
//! its refreshes, so every track was read and probed again. A track is read
//! again when its file changes, or when its album NFO's content does.
use crate::common;

use rustyfin_services::LibraryScanner;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

struct Tree(PathBuf);

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn nfo(first_title: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<album><title>Wet Tennis</title>\
         <track><position>1</position><title>{first_title}</title><file>01 Purple Hat.mp3</file></track>\
         <track><position>2</position><title>Summer</title><file>02 Summer.flac</file></track>\
         </album>\n"
    )
}

/// Write the album NFO and date it `secs` after now (a writer's refresh).
fn write_nfo(album: &std::path::Path, body: &str, secs: u64) {
    let p = album.join("album.nfo");
    std::fs::write(&p, body).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(secs))
        .unwrap();
}

#[tokio::test]
async fn a_rewritten_album_nfo_does_not_reread_its_tracks() {
    let app = common::TestApp::new().await;
    let root = std::env::temp_dir().join(format!("rf-rescan-{}", Uuid::new_v4().simple()));
    let tree = Tree(root.clone());
    let album = root.join("Sofi Tukker/Wet Tennis");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../rustyfin-media/tests/fixtures/audio");
    std::fs::copy(fixtures.join("sine.mp3"), album.join("01 Purple Hat.mp3")).unwrap();
    std::fs::copy(fixtures.join("sine.flac"), album.join("02 Summer.flac")).unwrap();
    write_nfo(&album, &nfo("Purple Hat"), 0);
    let paths = [tree.0.to_string_lossy().to_string()];
    let lib = rustyfin_data::repo::libraries::create(&app.pool, "Music", "music", &paths)
        .await
        .unwrap()
        .id;
    let scanner = LibraryScanner::new(app.data.clone(), None);

    let first = scanner.scan(lib, &paths).await.unwrap();
    assert_eq!(first.items_added, 2, "{first:?}");
    let again = scanner.scan(lib, &paths).await.unwrap();
    assert_eq!(
        (again.items_updated, again.items_skipped),
        (0, 2),
        "{again:?}"
    );

    // The NFO writer rewrites the same NFO: nothing is read again.
    write_nfo(&album, &nfo("Purple Hat"), 120);
    let rewritten = scanner.scan(lib, &paths).await.unwrap();
    assert_eq!(
        (rewritten.items_updated, rewritten.items_skipped),
        (0, 2),
        "same NFO content: {rewritten:?}"
    );

    // A changed NFO is read: the track takes its new title.
    write_nfo(&album, &nfo("Purple Hat (Remix)"), 240);
    let changed = scanner.scan(lib, &paths).await.unwrap();
    assert_eq!(changed.items_updated, 2, "{changed:?}");
    let title: String = sqlx::query_scalar(
        "SELECT name FROM media_items WHERE item_type = 'Audio' AND path LIKE '%01 Purple Hat.mp3'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(title, "Purple Hat (Remix)");
    let settled = scanner.scan(lib, &paths).await.unwrap();
    assert_eq!(settled.items_updated, 0, "{settled:?}");
    app.cleanup().await;
}
