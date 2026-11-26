//! `GET /Items/Counts`: how many items of each kind the caller may see.
use serde::{Deserialize, Serialize};

/// Jellyfin's `ItemCounts`. Every field is present, zero when there are none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemCounts {
    pub movie_count: i64,
    pub series_count: i64,
    pub episode_count: i64,
    pub artist_count: i64,
    pub program_count: i64,
    pub trailer_count: i64,
    pub song_count: i64,
    pub album_count: i64,
    pub music_video_count: i64,
    pub box_set_count: i64,
    pub book_count: i64,
    pub item_count: i64,
}

impl ItemCounts {
    /// Folds `(item_type, count)` rows into the named buckets. Types with no
    /// bucket (seasons, folders, playlists) still count towards `ItemCount`.
    pub fn from_type_counts<'a>(rows: impl IntoIterator<Item = (&'a str, i64)>) -> Self {
        let mut c = Self::default();
        for (item_type, n) in rows {
            match item_type {
                "Movie" => c.movie_count += n,
                "Series" => c.series_count += n,
                "Episode" => c.episode_count += n,
                "MusicArtist" => c.artist_count += n,
                "Program" => c.program_count += n,
                "Trailer" => c.trailer_count += n,
                "Audio" => c.song_count += n,
                "MusicAlbum" => c.album_count += n,
                "MusicVideo" => c.music_video_count += n,
                "BoxSet" => c.box_set_count += n,
                "Book" => c.book_count += n,
                _ => {}
            }
            c.item_count += n;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_by_type_and_totals_everything() {
        let c = ItemCounts::from_type_counts([
            ("Movie", 2),
            ("Episode", 5),
            ("Season", 1),
            ("Audio", 3),
        ]);
        assert_eq!(c.movie_count, 2);
        assert_eq!(c.episode_count, 5);
        assert_eq!(c.song_count, 3);
        assert_eq!(c.item_count, 11);
        assert_eq!(c.series_count, 0);
    }

    #[test]
    fn serializes_every_field_in_pascal_case() {
        let v = serde_json::to_value(ItemCounts::default()).unwrap();
        for k in [
            "MovieCount",
            "SeriesCount",
            "EpisodeCount",
            "ArtistCount",
            "ProgramCount",
            "TrailerCount",
            "SongCount",
            "AlbumCount",
            "MusicVideoCount",
            "BoxSetCount",
            "BookCount",
            "ItemCount",
        ] {
            assert_eq!(v[k], 0, "{k}");
        }
    }
}
