//! What playback reports change: the user's played state and resume point
//! ([`crate::play_state`]'s rules) and the play history; and who may read
//! that history. Report handling never fails playback: failures are logged.

use rustyfin_models::data::{
    Data, MediaItemStore, Play, PlayFilter, PlayHistoryStore, PlayReport, UserDataStore,
};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};
use crate::play_state::{self, Kind};

/// The most plays one page of history holds.
pub const MAX_HISTORY_PAGE: i64 = 500;

/// What a report says happened to the played state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Playback started.
    Start,
    /// Playback is at (or stopped at) this position.
    Position(i64),
    /// Playback stopped and the client did not say where.
    StopWithoutPosition,
}

/// A start or progress report, for the play history.
#[derive(Debug, Clone, Copy)]
pub struct Heard<'a> {
    pub user: Uuid,
    /// The client session reporting.
    pub session: Uuid,
    pub item: Uuid,
    /// The client's play session id, when it sends one.
    pub play_session: Option<&'a str>,
    pub paused: bool,
    pub position_ticks: Option<i64>,
    pub play_method: Option<&'a str>,
    /// The app and device of the session, when known.
    pub client: Option<&'a str>,
    pub device_name: Option<&'a str>,
}

/// Who is asking, for reads other users' data could answer.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    pub user: Uuid,
    pub is_administrator: bool,
}

impl Viewer {
    /// Administrators see anyone's; a user sees their own.
    fn may_see(self, user: Uuid) -> ServiceResult<()> {
        if self.is_administrator || self.user == user {
            Ok(())
        } else {
            Err(ServiceError::Forbidden)
        }
    }
}

/// Minutes and plays of one kind of media.
#[derive(Debug, Clone, PartialEq)]
pub struct KindStats {
    pub kind: String,
    pub millis: i64,
    pub plays: i64,
}

/// The key of a play: the client's play session id, else its session and
/// the item.
pub fn play_key(session: Uuid, play_session: Option<&str>, item: Uuid) -> String {
    match play_session.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => format!("{session}:{item}"),
    }
}

/// Playback rules over a data layer.
pub struct Playback<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Playback<'_, D> {
    /// Apply a report to the user's played state for the item.
    pub async fn progress(&self, user: Uuid, item: Uuid, report: Progress) {
        let found = match self.data.media_items().find_by_id(item).await {
            Ok(Some(found)) => found,
            _ => return,
        };
        let kind = Kind::of(
            &found.item_type,
            found.is_folder,
            found.extra_type.as_deref(),
        );
        let current = match self.data.user_data().get(user, item).await {
            Ok(Some(ud)) => play_state::State {
                played: ud.played,
                position_ticks: ud.playback_position_ticks,
                play_count: ud.play_count,
            },
            _ => play_state::State::default(),
        };
        let (next, stamp) = match report {
            Progress::Start => (play_state::on_start(current, kind), true),
            Progress::Position(position) => {
                let (next, _) =
                    play_state::on_position(current, kind, position, found.runtime_ticks);
                (next, false)
            }
            Progress::StopWithoutPosition => {
                (play_state::on_stop_without_position(current, kind), true)
            }
        };
        if let Err(e) = self
            .data
            .user_data()
            .save_play_state(
                user,
                item,
                next.played,
                next.position_ticks,
                next.play_count,
                stamp,
            )
            .await
        {
            tracing::warn!("Play state for {item}: {e}");
        }
    }

    /// Record a start or progress report in the history.
    pub async fn heard(&self, h: Heard<'_>) {
        let Ok(Some(found)) = self.data.media_items().find_by_id(h.item).await else {
            return;
        };
        let parent = match found.item_type.as_str() {
            "Episode" => found.series_name.clone(),
            "Audio" => found
                .artists
                .as_ref()
                .filter(|a| !a.is_empty())
                .map(|a| a.join(", ")),
            _ => None,
        };
        let key = play_key(h.session, h.play_session, h.item);
        let report = PlayReport {
            user_id: h.user,
            play_key: &key,
            item_id: h.item,
            item_type: &found.item_type,
            item_name: &found.name,
            parent_name: parent.as_deref(),
            client: h.client,
            device_name: h.device_name,
            play_method: h.play_method,
            paused: h.paused,
            position_ticks: h.position_ticks,
        };
        if let Err(e) = self.data.play_history().report(&report).await {
            tracing::warn!("Play history: {e}");
        }
    }

    /// Record a stop in the history.
    pub async fn stopped(
        &self,
        user: Uuid,
        session: Uuid,
        item: Uuid,
        play_session: Option<&str>,
        position_ticks: Option<i64>,
    ) {
        let key = play_key(session, play_session, item);
        if let Err(e) = self
            .data
            .play_history()
            .stop(user, &key, position_ticks)
            .await
        {
            tracing::warn!("Play history: {e}");
        }
    }

    /// A user's time and plays per kind of media.
    pub async fn stats(&self, viewer: Viewer, user: Uuid) -> ServiceResult<Vec<KindStats>> {
        viewer.may_see(user)?;
        Ok(self
            .data
            .play_history()
            .stats(user)
            .await?
            .into_iter()
            .map(|(kind, millis, plays)| KindStats {
                kind,
                millis,
                plays,
            })
            .collect())
    }

    /// A page of a user's plays, newest first, and how many match. `limit`
    /// is kept between 1 and [`MAX_HISTORY_PAGE`].
    pub async fn history(
        &self,
        viewer: Viewer,
        user: Uuid,
        filter: &PlayFilter,
        start: i64,
        limit: i64,
    ) -> ServiceResult<(Vec<Play>, i64)> {
        viewer.may_see(user)?;
        Ok(self
            .data
            .play_history()
            .list(user, filter, start.max(0), limit.clamp(1, MAX_HISTORY_PAGE))
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_play_is_keyed_by_the_play_session_else_session_and_item() {
        let (s, i) = (Uuid::nil(), Uuid::max());
        assert_eq!(play_key(s, Some(" abc "), i), "abc");
        assert_eq!(play_key(s, Some("  "), i), format!("{s}:{i}"));
        assert_eq!(play_key(s, None, i), format!("{s}:{i}"));
    }

    #[test]
    fn only_administrators_see_other_users_history() {
        let me = Uuid::nil();
        let other = Uuid::max();
        let user = Viewer {
            user: me,
            is_administrator: false,
        };
        assert!(user.may_see(me).is_ok());
        assert!(matches!(user.may_see(other), Err(ServiceError::Forbidden)));
        let admin = Viewer {
            is_administrator: true,
            ..user
        };
        assert!(admin.may_see(other).is_ok());
    }
}
