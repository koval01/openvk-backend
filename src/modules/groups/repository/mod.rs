use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::db::entities::group::{self, Entity as GroupEntity};
use crate::db::entities::group_member::{self, Entity as GroupMemberEntity};
use crate::error::AppError;
use crate::modules::groups::models::Group;
use crate::modules::media::kinds::public_media_url;

pub struct GroupRepository<'a> {
    db: &'a DatabaseConnection,
    media_base_url: &'a str,
}

impl<'a> GroupRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection, media_base_url: &'a str) -> Self {
        Self { db, media_base_url }
    }

    pub async fn list(&self) -> Result<Vec<Group>, AppError> {
        let rows = GroupEntity::find()
            .order_by_desc(group::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            groups.push(self.to_group(row).await?);
        }
        Ok(groups)
    }

    pub async fn get(&self, id: i64) -> Result<Option<Group>, AppError> {
        let Some(row) = GroupEntity::find_by_id(id).one(self.db).await? else {
            return Ok(None);
        };
        Ok(Some(self.to_group(row).await?))
    }

    async fn to_group(&self, row: group::Model) -> Result<Group, AppError> {
        let members = i64::try_from(
            GroupMemberEntity::find()
                .filter(group_member::Column::GroupId.eq(row.id))
                .filter(group_member::Column::Status.eq("active"))
                .count(self.db)
                .await?,
        )
        .unwrap_or(i64::MAX);
        Ok(Group {
            id: row.id,
            slug: row.slug,
            name: row.name,
            about: row.about,
            kind: row.kind,
            owner_id: row.owner_id,
            created_at: row.created_at,
            avatar_url: row
                .avatar_key
                .map(|key| public_media_url(self.media_base_url, &key)),
            members,
            wall_open: row.wall_open,
        })
    }

    pub async fn find_many(&self, ids: &[i64]) -> Result<Vec<Group>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = GroupEntity::find()
            .filter(group::Column::Id.is_in(ids.iter().copied()))
            .all(self.db)
            .await?;
        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            groups.push(self.to_group(row).await?);
        }
        Ok(groups)
    }

    pub async fn is_active_member(&self, group_id: i64, user_id: i64) -> Result<bool, AppError> {
        Ok(GroupMemberEntity::find()
            .filter(group_member::Column::GroupId.eq(group_id))
            .filter(group_member::Column::UserId.eq(user_id))
            .filter(group_member::Column::Status.eq("active"))
            .one(self.db)
            .await?
            .is_some())
    }
}
