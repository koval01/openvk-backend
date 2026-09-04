#![allow(dead_code)]

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::db::entities::feed_event::{self, Entity as FeedEventEntity};
use crate::error::AppError;

pub struct FeedRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> FeedRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn recent_event_ids(&self, limit: u64) -> Result<Vec<i64>, AppError> {
        let rows = FeedEventEntity::find()
            .order_by_desc(feed_event::Column::CreatedAt)
            .limit(limit)
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    pub async fn events_for_actors(
        &self,
        actor_ids: &[i64],
        limit: u64,
    ) -> Result<Vec<feed_event::Model>, AppError> {
        if actor_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(FeedEventEntity::find()
            .filter(feed_event::Column::ActorId.is_in(actor_ids.iter().copied()))
            .order_by_desc(feed_event::Column::CreatedAt)
            .limit(limit)
            .all(self.db)
            .await?)
    }
}
