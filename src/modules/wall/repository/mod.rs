use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
    TransactionTrait,
};

use crate::db::entities::comment::{self, Entity as CommentEntity};
use crate::db::entities::wall_attachment::{self, Entity as WallAttachmentEntity};
use crate::db::entities::wall_post::{self, Entity as WallPostEntity};
use crate::error::AppError;
use crate::modules::groups::models::Group;
use crate::modules::groups::repository::GroupRepository;
use crate::modules::users::repository::UserRepository;
use crate::modules::users::{PrivacyLevel, User};
use crate::modules::wall::models::{GeoPoint, WallAttachment, WallPost, WriteWall};
use crate::vault::Vault;

pub struct WallPostRepository<'a> {
    db: &'a DatabaseConnection,
    vault: &'a Vault,
    media_base_url: &'a str,
}

impl<'a> WallPostRepository<'a> {
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

    pub async fn insert_user(
        &self,
        target: &User,
        author: &User,
        body: WriteWall,
    ) -> Result<WallPost, AppError> {
        self.insert_row(target.id, None, target, None, author, body)
            .await
    }

    pub async fn insert_group(
        &self,
        group: &Group,
        author: &User,
        body: WriteWall,
    ) -> Result<WallPost, AppError> {
        let target = user_from_club(group);
        self.insert_row(
            -group.id,
            Some(group.id),
            &target,
            Some(group.clone()),
            author,
            body,
        )
        .await
    }

    async fn insert_row(
        &self,
        target_id: i64,
        group_id: Option<i64>,
        target: &User,
        club: Option<Group>,
        author: &User,
        body: WriteWall,
    ) -> Result<WallPost, AppError> {
        let txn = self.db.begin().await?;
        let local_id = next_wall_local_id(&txn, target_id).await?;
        let (geo_lat, geo_lng, geo_name) = match &body.geo {
            Some(geo) => (Some(geo.lat), Some(geo.lng), Some(geo.name.clone())),
            None => (None, None, None),
        };
        let model = wall_post::ActiveModel {
            target_id: Set(target_id),
            local_id: Set(local_id),
            author_id: Set(author.id),
            group_id: Set(group_id),
            content: Set(body.content.clone()),
            geo_lat: Set(geo_lat),
            geo_lng: Set(geo_lng),
            geo_name: Set(geo_name),
            source: Set(body.source.clone()),
            nsfw: Set(body.nsfw),
            created_at: Set(Utc::now()),
            ..Default::default()
        };
        let row = model.insert(&txn).await?;
        for (sort, attachment) in body.attachments.iter().enumerate() {
            let sort = i32::try_from(sort).unwrap_or(i32::MAX);
            wall_attachment::ActiveModel {
                post_id: Set(row.id),
                sort: Set(sort),
                kind: Set(attachment.kind.clone()),
                owner_id: Set(attachment.owner_id),
                object_id: Set(attachment.object_id),
                url: Set(attachment.url.clone()),
                title: Set(attachment.title.clone()),
                src: Set(attachment.src.clone()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }
        txn.commit().await?;
        let geo = geo_from_row(&row);
        Ok(WallPost::assemble(
            row.local_id,
            row.target_id,
            row.author_id,
            author.clone(),
            target.clone(),
            club,
            row.content,
            row.created_at,
            body.attachments,
            geo,
            row.source,
            row.nsfw,
            0,
        ))
    }

    async fn hydrate(&self, rows: Vec<wall_post::Model>) -> Result<Vec<WallPost>, AppError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let mut user_ids = Vec::new();
        let mut group_ids = Vec::new();
        for row in &rows {
            user_ids.push(row.author_id);
            if row.target_id > 0 {
                user_ids.push(row.target_id);
            } else if row.target_id < 0 {
                group_ids.push(-row.target_id);
            }
        }
        user_ids.sort_unstable();
        user_ids.dedup();
        group_ids.sort_unstable();
        group_ids.dedup();

        let users = UserRepository::new(self.db, self.vault, self.media_base_url)
            .find_many(&user_ids)
            .await?;
        let groups = GroupRepository::new(self.db, self.media_base_url)
            .find_many(&group_ids)
            .await?;

        let post_ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let attachment_rows = WallAttachmentEntity::find()
            .filter(wall_attachment::Column::PostId.is_in(post_ids.clone()))
            .order_by_asc(wall_attachment::Column::Sort)
            .all(self.db)
            .await?;
        let mut attachments: HashMap<i64, Vec<WallAttachment>> = HashMap::new();
        for row in attachment_rows {
            attachments
                .entry(row.post_id)
                .or_default()
                .push(WallAttachment {
                    kind: row.kind,
                    owner_id: row.owner_id,
                    object_id: row.object_id,
                    url: row.url,
                    title: row.title,
                    src: row.src,
                });
        }

        let comment_rows = CommentEntity::find()
            .filter(comment::Column::TargetKind.eq("wall"))
            .filter(comment::Column::DeletedAt.is_null())
            .filter(comment::Column::OwnerId.is_in(rows.iter().map(|row| row.target_id)))
            .all(self.db)
            .await?;
        let mut comment_counts: HashMap<(i64, i64), i32> = HashMap::new();
        for row in comment_rows {
            *comment_counts
                .entry((row.owner_id, row.object_id))
                .or_insert(0) += 1;
        }

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let author = users.iter().find(|user| user.id == row.author_id)?.clone();
                let (target, club) = if row.target_id < 0 {
                    let group = groups.iter().find(|group| group.id == -row.target_id)?;
                    (user_from_club(group), Some(group.clone()))
                } else {
                    let target = users.iter().find(|user| user.id == row.target_id)?;
                    (target.clone(), None)
                };
                let comment_count = comment_counts
                    .get(&(row.target_id, row.local_id))
                    .copied()
                    .unwrap_or(0);
                Some(WallPost::assemble(
                    row.local_id,
                    row.target_id,
                    row.author_id,
                    author,
                    target,
                    club,
                    row.content.clone(),
                    row.created_at,
                    attachments.remove(&row.id).unwrap_or_default(),
                    geo_from_row(&row),
                    row.source.clone(),
                    row.nsfw,
                    comment_count,
                ))
            })
            .collect())
    }
}

fn geo_from_row(row: &wall_post::Model) -> Option<GeoPoint> {
    let lat = row.geo_lat?;
    let lng = row.geo_lng?;
    Some(GeoPoint {
        lat,
        lng,
        name: row.geo_name.clone().unwrap_or_default(),
    })
}

pub fn user_from_club(group: &Group) -> User {
    User {
        id: -group.id,
        first_name: group.name.clone(),
        last_name: String::new(),
        screen_name: Some(group.slug.clone()),
        status: None,
        city: None,
        email: None,
        phone: None,
        avatar_url: group.avatar_url.clone(),
        verified: false,
        privacy_wall: PrivacyLevel::Everyone,
        privacy_messages: PrivacyLevel::Everyone,
        privacy_photos: PrivacyLevel::Everyone,
        privacy_audio: PrivacyLevel::Everyone,
        privacy_profile: PrivacyLevel::Everyone,
        privacy_friends: PrivacyLevel::Everyone,
        created_at: group.created_at,
        coins: 0,
        rating: 0,
        role: "user".into(),
        banned: false,
        ban_reason: None,
        banned_until: None,
        support_banned: false,
        support_ban_reason: None,
        posting_allowed: true,
        messaging_allowed: true,
    }
}

async fn next_wall_local_id<C: ConnectionTrait>(db: &C, target_id: i64) -> Result<i64, AppError> {
    let (sql, id) = if target_id > 0 {
        (
            "UPDATE users SET wall_seq = wall_seq + 1 WHERE id = $1 RETURNING wall_seq",
            target_id,
        )
    } else {
        (
            "UPDATE groups SET wall_seq = wall_seq + 1 WHERE id = $1 RETURNING wall_seq",
            -target_id,
        )
    };
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [id.into()],
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

pub fn can_write_group_wall(author_id: i64, group: &Group, is_member: bool) -> bool {
    group.wall_open || group.owner_id == author_id || is_member
}
