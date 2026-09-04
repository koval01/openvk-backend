use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

use crate::db::entities::comment::{self, Entity as CommentEntity};
use crate::error::AppError;
use crate::ids::insert_with_random_id;
use crate::modules::comments::models::{Comment, CommentTarget};
use crate::modules::users::repository::UserRepository;
use crate::vault::Vault;

pub struct CommentRepository<'a> {
    db: &'a DatabaseConnection,
    vault: &'a Vault,
    media_base_url: &'a str,
}

impl<'a> CommentRepository<'a> {
    pub const fn new(
        db: &'a DatabaseConnection,
        vault: &'a Vault,
        media_base_url: &'a str,
    ) -> Self {
        Self {
            db,
            vault,
            media_base_url,
        }
    }

    pub async fn list(
        &self,
        target: CommentTarget,
        owner_id: i64,
        object_id: i64,
    ) -> Result<Vec<Comment>, AppError> {
        let rows = CommentEntity::find()
            .filter(comment::Column::TargetKind.eq(target.as_str()))
            .filter(comment::Column::OwnerId.eq(owner_id))
            .filter(comment::Column::ObjectId.eq(object_id))
            .filter(comment::Column::DeletedAt.is_null())
            .order_by_asc(comment::Column::CreatedAt)
            .limit(100)
            .all(self.db)
            .await?;
        self.hydrate(rows).await
    }

    pub async fn get(&self, id: i64) -> Result<Option<Comment>, AppError> {
        let Some(row) = CommentEntity::find_by_id(id)
            .filter(comment::Column::DeletedAt.is_null())
            .one(self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(self.hydrate(vec![row]).await?.into_iter().next())
    }

    pub async fn insert(
        &self,
        target: CommentTarget,
        owner_id: i64,
        object_id: i64,
        author: &crate::modules::users::User,
        content: String,
    ) -> Result<Comment, AppError> {
        let created_at = Utc::now();
        let row = insert_with_random_id(|id| {
            comment::ActiveModel {
                id: Set(id),
                target_kind: Set(target.as_str().to_owned()),
                owner_id: Set(owner_id),
                object_id: Set(object_id),
                author_id: Set(author.id),
                content: Set(content.clone()),
                created_at: Set(created_at),
                deleted_at: Set(None),
            }
            .insert(self.db)
        })
        .await?;
        Ok(Comment {
            id: row.id,
            author_id: author.id,
            author: author.clone(),
            content: row.content,
            created_at: row.created_at,
            like_count: 0,
            liked: false,
        })
    }

    async fn hydrate(&self, rows: Vec<comment::Model>) -> Result<Vec<Comment>, AppError> {
        let mut ids: Vec<i64> = rows.iter().map(|row| row.author_id).collect();
        ids.sort_unstable();
        ids.dedup();
        let users = UserRepository::new(self.db, self.vault, self.media_base_url)
            .find_many(&ids)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let author = users.iter().find(|user| user.id == row.author_id)?.clone();
                Some(Comment {
                    id: row.id,
                    author_id: row.author_id,
                    author,
                    content: row.content,
                    created_at: row.created_at,
                    like_count: 0,
                    liked: false,
                })
            })
            .collect())
    }
}
