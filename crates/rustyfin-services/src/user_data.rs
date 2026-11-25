//! A user's own state on an item: played, play count, resume position,
//! favourite and rating. Every call first checks the user may see the item.

use rustyfin_models::data::{Data, UserData, UserDataChange, UserDataStore};
use uuid::Uuid;

use crate::access::Access;
use crate::error::ServiceResult;

/// A rating at or above this counts as a "like".
pub const MIN_LIKE_RATING: f64 = 6.5;

/// The rating a like or a dislike stands for.
pub fn rating_for_like(likes: bool) -> f64 {
    if likes { 10.0 } else { 1.0 }
}

/// Whether a rating counts as a like.
pub fn likes(rating: Option<f64>) -> Option<bool> {
    rating.map(|r| r >= MIN_LIKE_RATING)
}

/// What a client may change: each field given replaces what is there. A
/// like without a rating sets the rating it stands for.
#[derive(Debug, Clone, Default)]
pub struct UserDataUpdate {
    pub played: Option<bool>,
    pub play_count: Option<i32>,
    pub position_ticks: Option<i64>,
    pub is_favorite: Option<bool>,
    pub rating: Option<f64>,
    pub likes: Option<bool>,
    pub last_played_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// User data rules over a data layer.
pub struct UserItemData<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> UserItemData<'_, D> {
    /// The user's data on an item; `None` when they never touched it.
    pub async fn get(&self, user: Uuid, item: Uuid) -> ServiceResult<Option<UserData>> {
        self.check(user, item).await?;
        Ok(self.data.user_data().get(user, item).await?)
    }

    /// Change the fields given.
    pub async fn update(
        &self,
        user: Uuid,
        item: Uuid,
        update: UserDataUpdate,
    ) -> ServiceResult<UserData> {
        self.check(user, item).await?;
        let change = UserDataChange {
            played: update.played,
            play_count: update.play_count,
            position_ticks: update.position_ticks,
            is_favorite: update.is_favorite,
            rating: update.rating.or(update.likes.map(rating_for_like)),
            last_played_date: update.last_played_date,
        };
        Ok(self
            .data
            .user_data()
            .update_partial(user, item, &change)
            .await?)
    }

    /// Mark or unmark a favourite.
    pub async fn set_favorite(&self, user: Uuid, item: Uuid, on: bool) -> ServiceResult<UserData> {
        self.check(user, item).await?;
        Ok(self.data.user_data().set_favorite(user, item, on).await?)
    }

    /// Mark played or unplayed.
    pub async fn set_played(&self, user: Uuid, item: Uuid, on: bool) -> ServiceResult<UserData> {
        self.check(user, item).await?;
        Ok(self.data.user_data().set_played(user, item, on).await?)
    }

    /// Like, dislike, or (`None`) clear the rating.
    pub async fn set_like(
        &self,
        user: Uuid,
        item: Uuid,
        likes: Option<bool>,
    ) -> ServiceResult<UserData> {
        self.check(user, item).await?;
        Ok(self
            .data
            .user_data()
            .set_rating(user, item, likes.map(rating_for_like))
            .await?)
    }

    async fn check(&self, user: Uuid, item: Uuid) -> ServiceResult<()> {
        Access { data: self.data }.item(user, item).await.map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_like_is_a_high_rating_and_a_dislike_a_low_one() {
        assert_eq!(likes(Some(rating_for_like(true))), Some(true));
        assert_eq!(likes(Some(rating_for_like(false))), Some(false));
        assert_eq!(likes(Some(MIN_LIKE_RATING)), Some(true));
        assert_eq!(likes(None), None);
    }
}
