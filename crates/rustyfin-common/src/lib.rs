//! Pieces shared by the server and the `rustyfin` metadata tool.
//!
//! File and folder naming rules ([`naming`]), Kodi/Radarr NFO reading
//! ([`nfo`]), per-title `.rustyfin.toml` settings ([`title_settings`]),
//! music tags ([`music_tags`]), and the outside metadata sources with their
//! rate limits and caches: TMDB, TVDB, fanart.tv, MusicBrainz, Discogs and
//! LRCLIB lyrics ([`tmdb`], [`tvdb`], [`fanart`], [`musicbrainz`],
//! [`discogs`], [`lyrics`], [`rate_limit`], [`cache`]), plus artwork and
//! image helpers ([`artwork`], [`images`]).

pub mod artwork;
pub mod cache;
pub mod discogs;
pub mod fanart;
pub mod images;
pub mod lyrics;
pub mod music_tags;
pub mod musicbrainz;
pub mod naming;
pub mod nfo;
pub mod rate_limit;
pub mod title_settings;
pub mod tmdb;
pub mod tvdb;
