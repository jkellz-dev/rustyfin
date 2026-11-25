//! Who is asking, for rules that depend on it.

use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

/// The user a request comes from, and whether they administer the server.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    pub user: Uuid,
    pub is_administrator: bool,
}

impl Viewer {
    /// Administrators act for anyone; a user only for themself.
    pub fn may_act_for(self, user: Uuid) -> ServiceResult<()> {
        if self.is_administrator || self.user == user {
            Ok(())
        } else {
            Err(ServiceError::Forbidden)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_administrators_act_for_other_users() {
        let me = Uuid::nil();
        let other = Uuid::max();
        let user = Viewer {
            user: me,
            is_administrator: false,
        };
        assert!(user.may_act_for(me).is_ok());
        assert!(matches!(
            user.may_act_for(other),
            Err(ServiceError::Forbidden)
        ));
        let admin = Viewer {
            is_administrator: true,
            ..user
        };
        assert!(admin.may_act_for(other).is_ok());
    }
}
