//! Jellyfin's remote-control shapes to and from the session service's
//! commands: requests arrive as Jellyfin's, travel as the service's, and
//! reach the listening app as Jellyfin's again.

use rustyfin_models::dto::{
    GeneralCommand, MessageCommand, PlayCommand, PlayRequest, PlaystateCommand, PlaystateRequest,
    SessionCommand,
};
use rustyfin_services::session::command::{
    Command, Message, Named, Play, Playback, PlaybackAction, QueuePlace,
};

/// A Jellyfin command as the session service carries it.
pub fn to_service(cmd: SessionCommand) -> Command {
    match cmd {
        SessionCommand::Play(r) => Command::Play(Play {
            item_ids: r.item_ids,
            start_position_ticks: r.start_position_ticks,
            place: match r.play_command {
                PlayCommand::PlayNow => QueuePlace::Now,
                PlayCommand::PlayNext => QueuePlace::Next,
                PlayCommand::PlayLast => QueuePlace::Last,
            },
            media_source_id: r.media_source_id,
            audio_stream_index: r.audio_stream_index,
            subtitle_stream_index: r.subtitle_stream_index,
            start_index: r.start_index,
        }),
        SessionCommand::Playstate(r) => Command::Playback(Playback {
            action: match r.command {
                PlaystateCommand::Stop => PlaybackAction::Stop,
                PlaystateCommand::Pause => PlaybackAction::Pause,
                PlaystateCommand::Unpause => PlaybackAction::Unpause,
                PlaystateCommand::NextTrack => PlaybackAction::NextTrack,
                PlaystateCommand::PreviousTrack => PlaybackAction::PreviousTrack,
                PlaystateCommand::Seek => PlaybackAction::Seek,
                PlaystateCommand::Rewind => PlaybackAction::Rewind,
                PlaystateCommand::FastForward => PlaybackAction::FastForward,
            },
            seek_position_ticks: r.seek_position_ticks,
            controlling_user_id: r.controlling_user_id,
        }),
        SessionCommand::GeneralCommand(c) => Command::Named(Named {
            name: c.name,
            arguments: c.arguments,
            controlling_user_id: c.controlling_user_id,
        }),
        SessionCommand::Message(m) => Command::Message(Message {
            header: m.header,
            text: m.text,
            timeout_ms: m.timeout_ms,
        }),
    }
}

/// A session service command in Jellyfin's shape, for the listening app.
pub fn from_service(cmd: Command) -> SessionCommand {
    match cmd {
        Command::Play(p) => SessionCommand::Play(PlayRequest {
            item_ids: p.item_ids,
            start_position_ticks: p.start_position_ticks,
            play_command: match p.place {
                QueuePlace::Now => PlayCommand::PlayNow,
                QueuePlace::Next => PlayCommand::PlayNext,
                QueuePlace::Last => PlayCommand::PlayLast,
            },
            media_source_id: p.media_source_id,
            audio_stream_index: p.audio_stream_index,
            subtitle_stream_index: p.subtitle_stream_index,
            start_index: p.start_index,
        }),
        Command::Playback(p) => SessionCommand::Playstate(PlaystateRequest {
            command: match p.action {
                PlaybackAction::Stop => PlaystateCommand::Stop,
                PlaybackAction::Pause => PlaystateCommand::Pause,
                PlaybackAction::Unpause => PlaystateCommand::Unpause,
                PlaybackAction::NextTrack => PlaystateCommand::NextTrack,
                PlaybackAction::PreviousTrack => PlaystateCommand::PreviousTrack,
                PlaybackAction::Seek => PlaystateCommand::Seek,
                PlaybackAction::Rewind => PlaystateCommand::Rewind,
                PlaybackAction::FastForward => PlaystateCommand::FastForward,
            },
            seek_position_ticks: p.seek_position_ticks,
            controlling_user_id: p.controlling_user_id,
        }),
        Command::Named(n) => SessionCommand::GeneralCommand(GeneralCommand {
            name: n.name,
            arguments: n.arguments,
            controlling_user_id: n.controlling_user_id,
        }),
        Command::Message(m) => SessionCommand::Message(MessageCommand {
            header: m.header,
            text: m.text,
            timeout_ms: m.timeout_ms,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn json(cmd: SessionCommand) -> serde_json::Value {
        match cmd {
            SessionCommand::Play(r) => serde_json::to_value(r),
            SessionCommand::Playstate(r) => serde_json::to_value(r),
            SessionCommand::GeneralCommand(c) => serde_json::to_value(c),
            SessionCommand::Message(m) => serde_json::to_value(m),
        }
        .unwrap()
    }

    /// What reaches the app is what was sent: the trip through the session
    /// service changes nothing in Jellyfin's message.
    #[test]
    fn a_command_crosses_the_service_unchanged() {
        let commands = vec![
            SessionCommand::Play(PlayRequest {
                item_ids: vec![Uuid::new_v4()],
                start_position_ticks: Some(5),
                play_command: PlayCommand::PlayNext,
                media_source_id: Some("m".into()),
                audio_stream_index: Some(1),
                subtitle_stream_index: Some(2),
                start_index: Some(3),
            }),
            SessionCommand::Playstate(PlaystateRequest {
                command: PlaystateCommand::Seek,
                seek_position_ticks: Some(9),
                controlling_user_id: Some("u".into()),
            }),
            SessionCommand::GeneralCommand(GeneralCommand {
                name: "SetVolume".into(),
                arguments: HashMap::from([("Volume".into(), "40".into())]),
                controlling_user_id: None,
            }),
            SessionCommand::Message(MessageCommand {
                header: Some("h".into()),
                text: "t".into(),
                timeout_ms: Some(1000),
            }),
        ];
        for cmd in commands {
            let before = json(cmd.clone());
            assert_eq!(json(from_service(to_service(cmd))), before);
        }
    }
}
