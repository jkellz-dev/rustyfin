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
    pub musicbrainz_album_artist_id: Option<String>,
    /// The release track (Jellyfin's `MusicBrainzTrack`), not the recording.
    pub musicbrainz_track_id: Option<String>,
    pub label: Option<String>,
    pub album_type: Option<String>,
    pub original_date: Option<String>,
    pub total_tracks: Option<u32>,
    pub total_discs: Option<u32>,
    pub barcode: Option<String>,
    pub catalog_number: Option<String>,
    pub release_country: Option<String>,
    /// Lyrics in the tags (USLT/SYLT, Vorbis LYRICS, ...).
    pub has_lyrics: bool,
}

pub fn read_tags(path: &Path) -> Option<AudioTags> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, ItemKey};

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    // lofty maps each format's own names onto these keys (Vorbis
    // `MUSICBRAINZ_ALBUMID`, `ORGANIZATION`, `TOTALTRACKS`, ID3v2 TXXX, MP4
    // freeform atoms, ...).
    let get = |key: ItemKey| tag.get_string(key).map(str::to_string);

    Some(AudioTags {
        has_lyrics: tag
            .get_string(ItemKey::Lyrics)
            .is_some_and(|l| !l.trim().is_empty()),
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: get(ItemKey::AlbumArtist),
        album: tag.album().map(|s| s.to_string()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.date().map(|d| i32::from(d.year)),
        genre: tag.genre().map(|s| s.to_string()),
        has_embedded_art: !tag.pictures().is_empty(),

        musicbrainz_artist_id: get(ItemKey::MusicBrainzArtistId),
        musicbrainz_album_id: get(ItemKey::MusicBrainzReleaseId),
        musicbrainz_release_group_id: get(ItemKey::MusicBrainzReleaseGroupId),
        musicbrainz_album_artist_id: get(ItemKey::MusicBrainzReleaseArtistId),
        musicbrainz_track_id: get(ItemKey::MusicBrainzTrackId),
        label: get(ItemKey::Label),
        album_type: get(ItemKey::MusicBrainzReleaseType),
        original_date: get(ItemKey::OriginalReleaseDate),
        total_tracks: tag.track_total(),
        total_discs: tag.disk_total(),
        barcode: get(ItemKey::Barcode),
        catalog_number: get(ItemKey::CatalogNumber),
        release_country: get(ItemKey::ReleaseCountry),
    })
}

/// Split a genre string on common delimiters (`;`, `/`, `,`, `\0`) into
/// individual genres, trimming whitespace.
pub fn split_genres(genre: &str) -> Vec<String> {
    genre
        .split([';', '/', ',', '\0'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
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

/// The artists in a tag value: split on `;`, `|` and NUL (ID3v2.4 multi-value),
/// trimmed, empties and repeats dropped, order kept. `/` is not a separator, so
/// `AC/DC` stays one artist.
pub fn split_artists(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in value.split([';', '|', '\0']) {
        let name = part.trim();
        if !name.is_empty() && !out.iter().any(|seen| seen == name) {
            out.push(name.to_string());
        }
    }
    out
}

#[cfg(test)]
mod split_artists_tests {
    use super::split_artists;

    #[test]
    fn separators_split_and_repeats_go() {
        assert_eq!(split_artists("A; B|C ;A"), vec!["A", "B", "C"]);
        assert_eq!(split_artists("One\0Two"), vec!["One", "Two"]);
    }

    #[test]
    fn a_slash_is_part_of_the_name() {
        assert_eq!(split_artists("AC/DC"), vec!["AC/DC"]);
        assert!(split_artists(" ; ").is_empty());
    }
}

/// Jellyfin's MusicBrainz provider ids from a file's tags: for the track, its
/// album and its album artist. Empty maps when the file carries none.
#[derive(Debug, Default, PartialEq)]
pub struct MusicBrainzIds {
    pub track: Vec<(&'static str, String)>,
    pub album: Vec<(&'static str, String)>,
    pub artist: Vec<(&'static str, String)>,
}

pub fn musicbrainz_ids(tags: &AudioTags) -> MusicBrainzIds {
    let pick = |pairs: &[(&'static str, &Option<String>)]| -> Vec<(&'static str, String)> {
        pairs
            .iter()
            .filter_map(|(k, v)| {
                v.as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| (*k, v.to_string()))
            })
            .collect()
    };
    let album_artist = tags
        .musicbrainz_album_artist_id
        .as_ref()
        .or(tags.musicbrainz_artist_id.as_ref())
        .cloned();
    MusicBrainzIds {
        track: pick(&[
            ("MusicBrainzAlbum", &tags.musicbrainz_album_id),
            (
                "MusicBrainzReleaseGroup",
                &tags.musicbrainz_release_group_id,
            ),
            ("MusicBrainzArtist", &tags.musicbrainz_artist_id),
            ("MusicBrainzAlbumArtist", &tags.musicbrainz_album_artist_id),
            ("MusicBrainzTrack", &tags.musicbrainz_track_id),
        ]),
        album: pick(&[
            ("MusicBrainzAlbum", &tags.musicbrainz_album_id),
            (
                "MusicBrainzReleaseGroup",
                &tags.musicbrainz_release_group_id,
            ),
            ("MusicBrainzAlbumArtist", &album_artist),
        ]),
        artist: pick(&[("MusicBrainzArtist", &album_artist)]),
    }
}

/// What LRCLIB needs to find a recording, read from a track's tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackQuery {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_secs: u64,
    /// The tags already carry lyrics.
    pub has_lyrics: bool,
}

/// The track's artist, title, album and length; None when the file cannot be
/// read or lacks any of them (LRCLIB needs all four to match a recording).
pub fn read_track_query(path: &Path) -> Option<TrackQuery> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::{Accessor, ItemKey};
    let file = lofty::read_from_path(path).ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag())?;
    let duration_secs = file.properties().duration().as_secs();
    let s = |v: Option<std::borrow::Cow<'_, str>>| {
        v.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
    };
    let artist =
        s(tag.artist()).or_else(|| s(tag.get_string(ItemKey::AlbumArtist).map(Into::into)))?;
    Some(TrackQuery {
        artist,
        title: s(tag.title())?,
        album: s(tag.album())?,
        duration_secs: (duration_secs > 0).then_some(duration_secs)?,
        has_lyrics: tag
            .get_string(ItemKey::Lyrics)
            .is_some_and(|l| !l.trim().is_empty()),
    })
}
