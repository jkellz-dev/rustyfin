use std::path::Path;

#[derive(Debug, Default)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub has_embedded_art: bool,

    // Extended fields from Vorbis comments / ID3 frames
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub label: Option<String>,
    pub album_type: Option<String>,
    pub original_date: Option<String>,
    pub total_tracks: Option<u32>,
    pub total_discs: Option<u32>,
    pub barcode: Option<String>,
    pub catalog_number: Option<String>,
    pub release_status: Option<String>,
    pub release_country: Option<String>,
}

pub fn read_tags(path: &Path) -> Option<AudioTags> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, ItemKey};

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    // Helper: try a built-in ItemKey first, then fall back to raw Vorbis comment key names
    let get = |keys: &[ItemKey]| -> Option<String> {
        for key in keys {
            if let Some(val) = tag.get_string(key) {
                return Some(val.to_string());
            }
        }
        None
    };

    let get_u32 =
        |keys: &[ItemKey]| -> Option<u32> { get(keys).and_then(|s| s.parse::<u32>().ok()) };

    Some(AudioTags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: tag.get_string(&ItemKey::AlbumArtist).map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.year().map(|y| y as i32),
        genre: tag.genre().map(|s| s.to_string()),
        has_embedded_art: !tag.pictures().is_empty(),

        musicbrainz_artist_id: get(&[
            ItemKey::MusicBrainzArtistId,
            ItemKey::Unknown("MUSICBRAINZ_ARTISTID".to_string()),
        ]),
        musicbrainz_album_id: get(&[
            ItemKey::MusicBrainzReleaseId,
            ItemKey::Unknown("MUSICBRAINZ_ALBUMID".to_string()),
        ]),
        musicbrainz_release_group_id: get(&[
            ItemKey::MusicBrainzReleaseGroupId,
            ItemKey::Unknown("MUSICBRAINZ_RELEASEGROUPID".to_string()),
        ]),
        label: get(&[
            ItemKey::Label,
            ItemKey::Unknown("LABEL".to_string()),
            ItemKey::Unknown("ORGANIZATION".to_string()),
        ]),
        album_type: get(&[
            ItemKey::Unknown("RELEASETYPE".to_string()),
            ItemKey::Unknown("MUSICBRAINZ_ALBUMTYPE".to_string()),
        ]),
        original_date: get(&[
            ItemKey::OriginalReleaseDate,
            ItemKey::Unknown("ORIGINALDATE".to_string()),
            ItemKey::Unknown("ORIGINALYEAR".to_string()),
        ]),
        total_tracks: tag.track_total().or_else(|| {
            get_u32(&[
                ItemKey::TrackTotal,
                ItemKey::Unknown("TOTALTRACKS".to_string()),
                ItemKey::Unknown("TRACKTOTAL".to_string()),
            ])
        }),
        total_discs: tag.disk_total().or_else(|| {
            get_u32(&[
                ItemKey::DiscTotal,
                ItemKey::Unknown("TOTALDISCS".to_string()),
                ItemKey::Unknown("DISCTOTAL".to_string()),
            ])
        }),
        barcode: get(&[ItemKey::Barcode, ItemKey::Unknown("BARCODE".to_string())]),
        catalog_number: get(&[
            ItemKey::CatalogNumber,
            ItemKey::Unknown("CATALOGNUMBER".to_string()),
        ]),
        release_status: get(&[
            ItemKey::Unknown("RELEASESTATUS".to_string()),
            ItemKey::Unknown("MUSICBRAINZ_ALBUMSTATUS".to_string()),
        ]),
        release_country: get(&[ItemKey::Unknown("RELEASECOUNTRY".to_string())]),
    })
}

pub fn extract_cover_art(audio_path: &Path, output_path: &Path) -> Option<()> {
    use lofty::file::TaggedFileExt;
    use lofty::picture::MimeType;

    let tagged = lofty::read_from_path(audio_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let pic = tag.pictures().first()?;
    let data = pic.data();

    let ext = match pic.mime_type() {
        Some(MimeType::Png) => "png",
        Some(MimeType::Bmp) => "bmp",
        _ => "jpg",
    };

    let output = if output_path.is_dir() {
        output_path.join(format!("cover.{ext}"))
    } else {
        output_path.to_path_buf()
    };

    std::fs::write(&output, data).ok()?;
    Some(())
}
