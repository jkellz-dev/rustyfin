use serde::{Deserialize, Serialize};

/// Media item types in the Jellyfin object model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "PascalCase")]
#[sqlx(type_name = "text", rename_all = "PascalCase")]
pub enum ItemType {
    Movie,
    Series,
    Season,
    Episode,
    Audio,
    MusicAlbum,
    MusicArtist,
    Photo,
    PhotoAlbum,
    Folder,
    CollectionFolder,
    BoxSet,
    Playlist,
    Video,
    Trailer,
    Book,
    Person,
    Genre,
    Studio,
    MusicGenre,
    MusicVideo,
    Channel,
    Program,
    Recording,
    LiveTvChannel,
    LiveTvProgram,
    UserRootFolder,
    UserView,
    AggregateFolder,
    BasePluginFolder,
    ManualPlaylistsFolder,
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{}", s)
    }
}

/// Library collection types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryType {
    #[serde(rename = "movies")]
    Movies,
    #[serde(rename = "tvshows")]
    TvShows,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "photos")]
    Photos,
    #[serde(rename = "books")]
    Books,
    #[serde(rename = "mixed")]
    Mixed,
}

impl std::fmt::Display for LibraryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Movies => write!(f, "movies"),
            Self::TvShows => write!(f, "tvshows"),
            Self::Music => write!(f, "music"),
            Self::Photos => write!(f, "photos"),
            Self::Books => write!(f, "books"),
            Self::Mixed => write!(f, "mixed"),
        }
    }
}

/// Image types used throughout Jellyfin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageType {
    Primary,
    Art,
    Backdrop,
    Banner,
    Logo,
    Thumb,
    Disc,
    Box,
    Screenshot,
    Menu,
    Chapter,
    BoxRear,
    Profile,
}

/// Media stream types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaStreamType {
    Video,
    Audio,
    Subtitle,
    Attachment,
}

/// Sort order direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Item fields that can be requested in API queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemFields {
    AirTime,
    CanDelete,
    CanDownload,
    ChannelInfo,
    Chapters,
    ChildCount,
    CumulativeRunTimeTicks,
    DateCreated,
    DateLastMediaAdded,
    DisplayPreferencesId,
    ExternalUrls,
    Genres,
    HomePageUrl,
    ItemCounts,
    MediaSources,
    MediaStreams,
    Overview,
    ParentId,
    Path,
    People,
    ProviderIds,
    PrimaryImageAspectRatio,
    SortName,
    Studios,
    Tags,
    Width,
    Height,
}

/// Item filter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemFilter {
    IsFolder,
    IsNotFolder,
    IsUnplayed,
    IsPlayed,
    IsFavorite,
    IsResumable,
    Likes,
    Dislikes,
    IsFavoriteOrLikes,
}

/// Subtitle playback mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum SubtitleMode {
    Default = 0,
    Always = 1,
    OnlyForced = 2,
    None = 3,
    Smart = 4,
}

/// Device authorization status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAuthStatus {
    Pending,
    Authorized,
    Expired,
    Denied,
}

/// Activity log severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogSeverity {
    Info,
    Warn,
    Error,
}

/// Person type (actor, director, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersonType {
    Actor,
    Director,
    Writer,
    Producer,
    Composer,
    Conductor,
    GuestStar,
    Lyricist,
    Arranger,
    Engineer,
    Mixer,
    Remixer,
}
