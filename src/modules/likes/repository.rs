use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

use crate::db::entities::like::{self, Entity as LikeEntity};
use crate::error::AppError;
use crate::modules::likes::models::{LikeFlags, LikeState, LikeTarget};
use crate::modules::users::User;
use crate::modules::users::repository::UserRepository;
use crate::state::AppState;

pub struct LikeRepository<'a> {
    state: &'a AppState,
}

impl<'a> LikeRepository<'a> {
    pub const fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn toggle(&self, origin: i64, target: LikeTarget) -> Result<LikeState, AppError> {
        let existing = LikeEntity::find()
            .filter(like::Column::Origin.eq(origin))
            .filter(like::Column::TargetKind.eq(target.kind.as_str()))
            .filter(like::Column::OwnerId.eq(target.owner_id))
            .filter(like::Column::ObjectId.eq(target.object_id))
            .one(&self.state.db)
            .await?;
        if let Some(row) = existing {
            LikeEntity::delete_by_id((row.origin, row.target_kind, row.owner_id, row.object_id))
                .exec(&self.state.db)
                .await?;
        } else {
            like::ActiveModel {
                origin: Set(origin),
                target_kind: Set(target.kind.as_str().to_owned()),
                owner_id: Set(target.owner_id),
                object_id: Set(target.object_id),
                created_at: Set(Utc::now()),
            }
            .insert(&self.state.db)
            .await?;
        }
        self.state_for(origin, target).await
    }

    pub async fn set(
        &self,
        origin: i64,
        target: LikeTarget,
        liked: bool,
    ) -> Result<LikeState, AppError> {
        let current = self.state_for(origin, target).await?;
        if current.liked == liked {
            return Ok(current);
        }
        self.toggle(origin, target).await
    }

    pub async fn state_for(&self, origin: i64, target: LikeTarget) -> Result<LikeState, AppError> {
        let count = LikeEntity::find()
            .filter(like::Column::TargetKind.eq(target.kind.as_str()))
            .filter(like::Column::OwnerId.eq(target.owner_id))
            .filter(like::Column::ObjectId.eq(target.object_id))
            .count(&self.state.db)
            .await?;
        let liked = LikeEntity::find()
            .filter(like::Column::Origin.eq(origin))
            .filter(like::Column::TargetKind.eq(target.kind.as_str()))
            .filter(like::Column::OwnerId.eq(target.owner_id))
            .filter(like::Column::ObjectId.eq(target.object_id))
            .one(&self.state.db)
            .await?
            .is_some();
        Ok(LikeState {
            liked,
            count: i32::try_from(count).unwrap_or(i32::MAX),
        })
    }

    pub async fn flags_for(
        &self,
        viewer: i64,
        kind: &str,
        keys: &[(i64, i64)],
    ) -> Result<HashMap<(i64, i64), LikeFlags>, AppError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let owners: Vec<i64> = keys.iter().map(|key| key.0).collect();
        let rows = LikeEntity::find()
            .filter(like::Column::TargetKind.eq(kind))
            .filter(like::Column::OwnerId.is_in(owners))
            .all(&self.state.db)
            .await?;
        let wanted: std::collections::HashSet<(i64, i64)> = keys.iter().copied().collect();
        let mut flags: HashMap<(i64, i64), LikeFlags> = HashMap::new();
        for row in rows {
            let key = (row.owner_id, row.object_id);
            if !wanted.contains(&key) {
                continue;
            }
            let entry = flags.entry(key).or_default();
            entry.count = entry.count.saturating_add(1);
            if row.origin == viewer {
                entry.liked = true;
            }
        }
        Ok(flags)
    }

    pub async fn likers(&self, target: LikeTarget) -> Result<Vec<User>, AppError> {
        let rows = LikeEntity::find()
            .filter(like::Column::TargetKind.eq(target.kind.as_str()))
            .filter(like::Column::OwnerId.eq(target.owner_id))
            .filter(like::Column::ObjectId.eq(target.object_id))
            .order_by_asc(like::Column::CreatedAt)
            .limit(100)
            .all(&self.state.db)
            .await?;
        let ids: Vec<i64> = rows.iter().map(|row| row.origin).collect();
        UserRepository::new(
            &self.state.db,
            &self.state.vault,
            self.state.media_base_url(),
        )
        .find_many(&ids)
        .await
    }
}
