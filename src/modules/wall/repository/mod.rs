use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
};

use crate::db::entities::wall_post::{self, Entity as WallPostEntity};
use crate::error::AppError;
use crate::modules::users::repository::UserRepository;
use crate::modules::users::{PrivacyLevel, User};
use crate::modules::wall::models::WallPost;
use crate::vault::Vault;

pub struct WallPostRepository<'a> {
    db: &'a DatabaseConnection,
    vault: &'a Vault,
}

impl<'a> WallPostRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection, vault: &'a Vault) -> Self {
        Self { db, vault }
    }

    pub async fn list_for_target(&self, target_id: i64) -> Result<Vec<WallPost>, AppError> {
        let rows = WallPostEntity::find()
            .filter(wall_post::Column::TargetId.eq(target_id))
            .filter(wall_post::Column::DeletedAt.is_null())
            .order_by_desc(wall_post::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        self.hydrate(rows).await
    }

    pub async fn get_for_target(
        &self,
        target_id: i64,
        local_id: i64,
    ) -> Result<Option<WallPost>, AppError> {
        let row = WallPostEntity::find()
            .filter(wall_post::Column::TargetId.eq(target_id))
            .filter(wall_post::Column::LocalId.eq(local_id))
            .filter(wall_post::Column::DeletedAt.is_null())
            .one(self.db)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(self.hydrate(vec![row]).await?.into_iter().next())
    }

    pub async fn list_news(&self, target_ids: &[i64]) -> Result<Vec<WallPost>, AppError> {
        if target_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = WallPostEntity::find()
            .filter(wall_post::Column::TargetId.is_in(target_ids.iter().copied()))
            .filter(wall_post::Column::DeletedAt.is_null())
            .order_by_desc(wall_post::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        self.hydrate(rows).await
    }

    pub async fn insert(
        &self,
        target: &User,
        author: &User,
        content: String,
    ) -> Result<WallPost, AppError> {
        let local_id = next_wall_local_id(self.db, target.id).await?;
        let model = wall_post::ActiveModel {
            target_id: Set(target.id),
            local_id: Set(local_id),
            author_id: Set(author.id),
            content: Set(content),
            created_at: Set(Utc::now()),
            ..Default::default()
        };
        let row = model.insert(self.db).await?;
        Ok(WallPost::from_row(
            row.local_id,
            row.target_id,
            row.author_id,
            author.clone(),
            target.clone(),
            row.content,
            row.created_at,
        ))
    }

    async fn hydrate(&self, rows: Vec<wall_post::Model>) -> Result<Vec<WallPost>, AppError> {
        let mut ids = Vec::new();
        for row in &rows {
            ids.push(row.author_id);
            ids.push(row.target_id);
        }
        ids.sort_unstable();
        ids.dedup();

        let users = UserRepository::new(self.db, self.vault)
            .find_many(&ids)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let author = users.iter().find(|user| user.id == row.author_id)?.clone();
                let target = users.iter().find(|user| user.id == row.target_id)?.clone();
                Some(WallPost::from_row(
                    row.local_id,
                    row.target_id,
                    row.author_id,
                    author,
                    target,
                    row.content,
                    row.created_at,
                ))
            })
            .collect())
    }
}

async fn next_wall_local_id<C: ConnectionTrait>(db: &C, target_id: i64) -> Result<i64, AppError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE users SET wall_seq = wall_seq + 1 WHERE id = $1 RETURNING wall_seq",
            [target_id.into()],
        ))
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.try_get_by_index::<i64>(0)?)
}

pub fn can_write_wall(author_id: i64, target: &User, is_friend: bool) -> bool {
    if author_id == target.id {
        return true;
    }
    match target.privacy_wall {
        PrivacyLevel::Everyone => true,
        PrivacyLevel::Friends => is_friend,
        PrivacyLevel::Nobody => false,
    }
}
