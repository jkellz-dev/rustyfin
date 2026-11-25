//! Played state from playback reports, by Jellyfin 10.10's rules
//! (`UserDataManager.UpdatePlayState`, `SessionManager.OnPlaybackStart` and
//! `OnPlaybackStopped`, default settings). Owner, 2026-11-17: an episode
//! that reaches its credits must leave Next Up.

/// Under this share of the runtime, no resume point is kept.
pub const MIN_RESUME_PCT: f64 = 5.0;
/// Over this share of the runtime (the credits), the item is played.
pub const MAX_RESUME_PCT: f64 = 90.0;
/// Items shorter than this are played once past `MIN_RESUME_PCT`.
pub const MIN_RESUME_DURATION_SECS: f64 = 300.0;

const TICKS_PER_SECOND: f64 = 10_000_000.0;

/// What a kind of item supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// Can be marked played (anything but folders).
    pub played_status: bool,
    /// Keeps a resume point: videos that are not extras. Music does not.
    pub resumes: bool,
}

impl Kind {
    /// From an item's type, folder flag and extra type (`Version` rows are
    /// films; trailers and other extras do not resume, as in Jellyfin).
    pub fn of(item_type: &str, is_folder: bool, extra_type: Option<&str>) -> Kind {
        let extra = extra_type.is_some_and(|e| e != "Version");
        let video = matches!(
            item_type,
            "Movie" | "Episode" | "Video" | "MusicVideo" | "Trailer"
        );
        Kind {
            played_status: !is_folder,
            resumes: video && !extra && item_type != "Trailer",
        }
    }
}

/// A user's play state for one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    pub played: bool,
    pub position_ticks: i64,
    pub play_count: i32,
}

/// Playback started: one more play; what resumes is unplayed until its end
/// is reached again, what does not (music) is played at once.
pub fn on_start(state: State, kind: Kind) -> State {
    State {
        play_count: state.play_count + 1,
        played: kind.played_status && !kind.resumes,
        ..state
    }
}

/// A reported position (progress or stop). Returns the new state and
/// whether it was played to completion.
pub fn on_position(state: State, kind: Kind, position: i64, runtime: Option<i64>) -> (State, bool) {
    let runtime = runtime.unwrap_or(0);
    let mut next = state;
    let mut completed = false;
    let mut position = position.max(0);
    if runtime <= 0 {
        // The runtime is unknown: assume it was played.
        next.played = true;
        completed = true;
        position = 0;
    } else if position > 0 {
        let pct = position as f64 * 100.0 / runtime as f64;
        if pct < MIN_RESUME_PCT {
            position = 0;
        } else if pct > MAX_RESUME_PCT
            || position >= runtime
            || (runtime as f64 / TICKS_PER_SECOND) < MIN_RESUME_DURATION_SECS
        {
            // Past the credits, or too short to resume.
            position = 0;
            next.played = true;
            completed = true;
        }
    }
    if !kind.played_status {
        next.played = false;
    }
    next.position_ticks = if kind.resumes { position } else { 0 };
    (next, completed)
}

/// Playback stopped without a position: assume it was played.
pub fn on_stop_without_position(state: State, kind: Kind) -> State {
    State {
        play_count: state.play_count + 1,
        played: kind.played_status,
        position_ticks: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 600_000_000;
    const EPISODE: Kind = Kind {
        played_status: true,
        resumes: true,
    };

    #[test]
    fn the_credits_count_as_played_and_reset_the_place() {
        let (s, done) = on_position(State::default(), EPISODE, 21 * MIN, Some(22 * MIN));
        assert!(done);
        assert_eq!((s.played, s.position_ticks), (true, 0));
    }

    #[test]
    fn the_boundaries_are_jellyfins() {
        // Exactly 90% is still a resume point; past it is played.
        let (s, _) = on_position(State::default(), EPISODE, 90 * MIN, Some(100 * MIN));
        assert_eq!((s.played, s.position_ticks), (false, 90 * MIN));
        let (s, _) = on_position(State::default(), EPISODE, 90 * MIN + 1, Some(100 * MIN));
        assert!(s.played);
        // Exactly 5% keeps the place; under it does not.
        let (s, _) = on_position(State::default(), EPISODE, 5 * MIN, Some(100 * MIN));
        assert_eq!(s.position_ticks, 5 * MIN);
        let (s, _) = on_position(State::default(), EPISODE, 5 * MIN - 1, Some(100 * MIN));
        assert_eq!((s.played, s.position_ticks), (false, 0));
    }

    #[test]
    fn a_later_report_never_unplays() {
        let played = State {
            played: true,
            ..State::default()
        };
        let (s, done) = on_position(played, EPISODE, 30 * MIN, Some(100 * MIN));
        assert!(s.played && !done);
    }

    #[test]
    fn kinds_follow_jellyfin() {
        assert_eq!(Kind::of("Episode", false, None), EPISODE);
        assert_eq!(Kind::of("Movie", false, Some("Version")), EPISODE);
        assert!(!Kind::of("Movie", false, Some("Trailer")).resumes);
        assert!(!Kind::of("Audio", false, None).resumes);
        assert!(!Kind::of("Series", true, None).played_status);
        let track = on_start(State::default(), Kind::of("Audio", false, None));
        assert_eq!((track.played, track.play_count), (true, 1));
        let rewatch = on_start(
            State {
                played: true,
                play_count: 1,
                position_ticks: 0,
            },
            EPISODE,
        );
        assert_eq!((rewatch.played, rewatch.play_count), (false, 2));
    }
}
