use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder, QuerySelect};

use crate::db::entities::group::{self, Entity as GroupEntity};
use crate::error::AppError;
use crate::modules::groups::models::Group;

pub struct GroupRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> GroupRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<Group>, AppError> {
        let rows = GroupEntity::find()
            .order_by_desc(group::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Group {
                id: row.id,
                slug: row.slug,
                name: row.name,
                about: row.about,
                kind: row.kind,
                owner_id: row.owner_id,
                created_at: row.created_at,
            })
            .collect())
    }
}
