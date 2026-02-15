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
}

pub fn read_tags(path: &Path) -> Option<AudioTags> {
    use lofty::file::TaggedFileExt;
    use lofty::tag::Accessor;

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    Some(AudioTags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album_artist: tag
            .get_string(&lofty::tag::ItemKey::AlbumArtist)
            .map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.year().map(|y| y as i32),
        genre: tag.genre().map(|s| s.to_string()),
        has_embedded_art: !tag.pictures().is_empty(),
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
