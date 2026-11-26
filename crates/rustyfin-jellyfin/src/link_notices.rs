//! Telling users a friend's server went down or came back (owner,
//! 2026-11-19: "When a friends server is down, the user should be
//! notified"). Every session of a user who can see that link's titles gets
//! Jellyfin's `DisplayMessage` command, which apps show as a message.

use rustyfin_models::data::{Data, DataResult, LinkStore, UserStore};
use std::collections::HashMap;

use crate::dto::{GeneralCommand, SessionCommand};
use rustyfin_services::link::LinkEvent;
use uuid::Uuid;

use crate::router::AppState;

/// How long apps show the message.
const SHOW_MS: u32 = 10_000;

/// Listen for links going down and coming back, for as long as the server runs.
pub fn spawn<D: Data>(state: AppState<D>) {
    let mut events = state.links.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => tell(&state, &event).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// The message for an event: (header, text).
pub fn message(event: &LinkEvent) -> (String, String) {
    let name = &event.name;
    if event.online {
        (
            "Linked server back".into(),
            format!("{name}'s server is back. Its titles are available again."),
        )
    } else {
        (
            "Linked server down".into(),
            format!("{name}'s server can't be reached. Its titles are hidden until it is back."),
        )
    }
}

async fn tell<D: Data>(state: &AppState<D>, event: &LinkEvent) {
    let users = match who_sees(state, event.link_id).await {
        Ok(users) => users,
        Err(e) => {
            tracing::warn!("Could not tell users about link {}: {e}", event.name);
            return;
        }
    };
    let (header, text) = message(event);
    let command = GeneralCommand {
        name: "DisplayMessage".into(),
        arguments: HashMap::from([
            ("Header".to_string(), header),
            ("Text".to_string(), text),
            ("TimeoutMs".to_string(), SHOW_MS.to_string()),
        ]),
        controlling_user_id: None,
    };
    for session in state.session_manager.get_sessions(None).await {
        if users.contains(&session.user_id) {
            let _ = state
                .session_manager
                .send_command(
                    session.session_id,
                    crate::remote::to_service(SessionCommand::GeneralCommand(command.clone())),
                )
                .await;
        }
    }
}

/// Users who can see a library holding the link's titles.
async fn who_sees<D: Data>(state: &AppState<D>, link_id: Uuid) -> DataResult<Vec<Uuid>> {
    let libraries = state.data.links().libraries_of(link_id).await?;
    state.data.users().seeing_libraries(&libraries).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_friend_and_says_what_changed() {
        let mut event = LinkEvent {
            link_id: Uuid::nil(),
            name: "Alex".into(),
            online: false,
        };
        let (_, text) = message(&event);
        assert!(text.contains("Alex") && text.contains("hidden"), "{text}");
        event.online = true;
        let (_, text) = message(&event);
        assert!(text.contains("Alex") && text.contains("back"), "{text}");
    }
}
