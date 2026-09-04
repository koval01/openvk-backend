use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::db::entities::friendship::{self, Entity as FriendshipEntity};
use crate::error::AppError;

pub struct FriendshipRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> FriendshipRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn friend_ids(&self, user_id: i64) -> Result<Vec<i64>, AppError> {
        let rows = FriendshipEntity::find()
            .filter(friendship::Column::UserId.eq(user_id))
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|row| row.friend_id).collect())
    }

    pub async fn is_friend(&self, user_id: i64, other_id: i64) -> Result<bool, AppError> {
        Ok(FriendshipEntity::find()
            .filter(friendship::Column::UserId.eq(user_id))
            .filter(friendship::Column::FriendId.eq(other_id))
            .one(self.db)
            .await?
            .is_some())
    }
}
