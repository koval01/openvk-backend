use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::db::entities::notification::{self, Entity as NotificationEntity};
use crate::error::AppError;
use crate::modules::notifications::models::Notification;

pub struct NotificationRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> NotificationRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list_for_user(&self, user_id: i64) -> Result<Vec<Notification>, AppError> {
        let rows = NotificationEntity::find()
            .filter(notification::Column::UserId.eq(user_id))
            .order_by_desc(notification::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Notification {
                id: row.id,
                kind: row.kind,
                actor_id: row.actor_id,
                entity_type: row.entity_type,
                entity_id: row.entity_id,
                payload: row.payload,
                read_at: row.read_at,
                created_at: row.created_at,
            })
            .collect())
    }
}
