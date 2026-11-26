//! Users: reading them, their playback settings, administrators' changes
//! (policy, library access, deletion) and Rustyfin's own permissions.
//!
//! Who a user is comes from the proxy in front; nothing here signs anyone
//! in. A user changes their own settings; an administrator anyone's.

use rustyfin_models::data::{
    ActivityLogStore, Data, LibraryAccessStore, User, UserSettings, UserStore,
};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};
use crate::viewer::Viewer;

/// A user's playback settings as an app saves them.
#[derive(Debug, Clone, Default)]
pub struct SettingsChange {
    /// Blank means no preference.
    pub audio_language: Option<String>,
    /// Blank means no preference.
    pub subtitle_language: Option<String>,
    /// The subtitle mode by number (0 is the default mode).
    pub subtitle_mode: i32,
    pub play_default_audio_track: bool,
    pub next_episode_auto_play: bool,
    /// Everything else the app sent, kept as given.
    pub configuration: serde_json::Value,
}

/// An administrator's change to a user's role and library access.
#[derive(Debug, Clone, Default)]
pub struct PolicyChange {
    pub is_administrator: bool,
    pub all_libraries: bool,
    /// The libraries they may see when not all; replaces the list.
    pub libraries: Vec<Uuid>,
}

/// What a user may do beyond Jellyfin's policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    /// May link a server to this one.
    pub may_link: bool,
}

/// Who the proxy in front says is asking.
#[derive(Debug, Clone, Default)]
pub struct Vouched {
    /// The proxy's stable name for the person (the account's key).
    pub remote_user: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    /// Groups the proxy names; a member of the admin group administers.
    pub groups: Vec<String>,
}

/// User rules over a data layer.
pub struct Users<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Users<'_, D> {
    /// The account of someone the proxy vouches for, made on first sight;
    /// `None` when that account is disabled. A new server's first account
    /// administers it, and so does a member of `admin_group`.
    ///
    /// `refresh` also stores the name and email the proxy gives now (on a
    /// sign-in); without it an existing account is only read (every
    /// request).
    pub async fn vouched(
        &self,
        who: &Vouched,
        admin_group: Option<&str>,
        refresh: bool,
    ) -> ServiceResult<Option<User>> {
        let users = self.data.users();
        let existing = if refresh {
            None
        } else {
            users.find_by_remote_user_id(&who.remote_user).await?
        };
        let mut user = match existing {
            Some(user) => user,
            None => {
                let first = users.count().await? == 0;
                let user = users
                    .upsert_from_forward_auth(
                        &who.remote_user,
                        &who.remote_user,
                        who.display_name.as_deref(),
                        who.email.as_deref(),
                    )
                    .await?;
                if first && !user.is_administrator {
                    users
                        .update_policy(user.id, true, user.enable_all_libraries)
                        .await?;
                    users.find_by_id(user.id).await?.unwrap_or(user)
                } else {
                    user
                }
            }
        };
        if user.is_disabled {
            return Ok(None);
        }
        if !user.is_administrator && admin_group.is_some_and(|g| who.groups.iter().any(|x| x == g))
        {
            users
                .update_policy(user.id, true, user.enable_all_libraries)
                .await?;
            user.is_administrator = true;
        }
        Ok(Some(user))
    }

    /// Every user.
    pub async fn list(&self) -> ServiceResult<Vec<User>> {
        Ok(self.data.users().list_all().await?)
    }

    /// One user.
    pub async fn get(&self, id: Uuid) -> ServiceResult<User> {
        self.data
            .users()
            .find_by_id(id)
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// The libraries a user is limited to; `None` when they see them all.
    /// A failure to read the list is logged and answered with `None`.
    pub async fn enabled_libraries(&self, user: &User) -> Option<Vec<Uuid>> {
        if user.enable_all_libraries {
            return None;
        }
        match self
            .data
            .user_library_access()
            .get_enabled_libraries(user.id)
            .await
        {
            Ok(ids) => Some(ids),
            Err(e) => {
                tracing::warn!("libraries of user {}: {e}", user.id);
                None
            }
        }
    }

    /// Save a user's playback settings (the user, or an administrator).
    pub async fn save_settings(
        &self,
        viewer: Viewer,
        user: Uuid,
        change: &SettingsChange,
    ) -> ServiceResult<()> {
        viewer.may_act_for(user)?;
        let settings = UserSettings {
            audio_language_preference: non_blank(change.audio_language.as_deref()),
            subtitle_language_preference: non_blank(change.subtitle_language.as_deref()),
            subtitle_mode: change.subtitle_mode,
            play_default_audio_track: change.play_default_audio_track,
            enable_next_episode_auto_play: change.next_episode_auto_play,
            configuration: &change.configuration,
        };
        if !self.data.users().update_settings(user, &settings).await? {
            return Err(ServiceError::NotFound);
        }
        Ok(())
    }

    /// Delete a user; an administrator cannot delete themself.
    pub async fn delete(&self, admin: Uuid, user: Uuid) -> ServiceResult<()> {
        if admin == user {
            return Err(ServiceError::Invalid("Cannot delete yourself".into()));
        }
        if !self.data.users().delete(user).await? {
            return Err(ServiceError::NotFound);
        }
        let _ = self
            .data
            .activity_log()
            .insert(
                "User deleted",
                None,
                "UserDeleted",
                "Informational",
                Some(admin),
                None,
            )
            .await;
        Ok(())
    }

    /// Set a user's role and the libraries they may see.
    pub async fn set_policy(&self, user: Uuid, change: &PolicyChange) -> ServiceResult<()> {
        if !self
            .data
            .users()
            .update_policy(user, change.is_administrator, change.all_libraries)
            .await?
        {
            return Err(ServiceError::NotFound);
        }
        // The list is kept even when all libraries are allowed, so switching
        // back restores it.
        self.data
            .user_library_access()
            .set_enabled_libraries(user, &change.libraries)
            .await?;
        Ok(())
    }

    /// A user's permissions (the user, or an administrator).
    pub async fn permissions(&self, viewer: Viewer, user: Uuid) -> ServiceResult<Permissions> {
        viewer.may_act_for(user)?;
        self.current_permissions(user).await
    }

    /// Change what is given; answers with the permissions as they now are.
    pub async fn set_permissions(
        &self,
        user: Uuid,
        may_link: Option<bool>,
    ) -> ServiceResult<Permissions> {
        if let Some(may_link) = may_link
            && !self.data.users().set_may_link(user, may_link).await?
        {
            return Err(ServiceError::NotFound);
        }
        self.current_permissions(user).await
    }

    async fn current_permissions(&self, user: Uuid) -> ServiceResult<Permissions> {
        Ok(Permissions {
            may_link: self.get(user).await?.may_link,
        })
    }
}

fn non_blank(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}
