use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
};
use serde_json::{Value, json};

use crate::db::entities::notification::{self, Entity as NotificationEntity};
use crate::error::AppError;
use crate::modules::notifications::models::Notification;
use crate::modules::users::repository::UserRepository;
use crate::vault::Vault;

pub struct NotificationRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> NotificationRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list_for_user(
        &self,
        user_id: i64,
        vault: &Vault,
        media_base_url: &str,
    ) -> Result<Vec<Notification>, AppError> {
        let rows = NotificationEntity::find()
            .filter(notification::Column::UserId.eq(user_id))
            .order_by_desc(notification::Column::CreatedAt)
            .limit(50)
            .all(self.db)
            .await?;
        let mut actor_ids: Vec<i64> = rows.iter().filter_map(|row| row.actor_id).collect();
        actor_ids.sort_unstable();
        actor_ids.dedup();
        let actors = UserRepository::new(self.db, vault, media_base_url)
            .find_many(&actor_ids)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let href = href_from_payload(&row.payload)
                    .unwrap_or_else(|| href_from_entity(row.entity_type.as_deref(), row.entity_id));
                let actor = row
                    .actor_id
                    .and_then(|id| actors.iter().find(|user| user.id == id).cloned());
                Notification {
                    id: row.id,
                    kind: row.kind,
                    actor_id: row.actor_id,
                    actor,
                    entity_type: row.entity_type,
                    entity_id: row.entity_id,
                    payload: row.payload,
                    href,
                    read_at: row.read_at,
                    created_at: row.created_at,
                }
            })
            .collect())
    }

    pub async fn insert_like(
        &self,
        user_id: i64,
        actor_id: i64,
        entity_type: &str,
        entity_id: i64,
        href: &str,
    ) -> Result<(), AppError> {
        notification::ActiveModel {
            user_id: Set(user_id),
            kind: Set("like".into()),
            actor_id: Set(Some(actor_id)),
            entity_type: Set(Some(entity_type.to_owned())),
            entity_id: Set(Some(entity_id)),
            payload: Set(json!({ "href": href })),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(self.db)
        .await?;
        Ok(())
    }

    pub async fn insert_comment(
        &self,
        user_id: i64,
        actor_id: i64,
        entity_type: &str,
        entity_id: i64,
        href: &str,
    ) -> Result<(), AppError> {
        notification::ActiveModel {
            user_id: Set(user_id),
            kind: Set("comment".into()),
            actor_id: Set(Some(actor_id)),
            entity_type: Set(Some(entity_type.to_owned())),
            entity_id: Set(Some(entity_id)),
            payload: Set(json!({ "href": href })),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(self.db)
        .await?;
        Ok(())
    }

    pub async fn mark_seen(&self, user_id: i64) -> Result<(), AppError> {
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE notifications SET read_at = NOW() WHERE user_id = $1 AND read_at IS NULL",
                [user_id.into()],
            ))
            .await?;
        Ok(())
    }
}

fn href_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("href")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn href_from_entity(entity_type: Option<&str>, entity_id: Option<i64>) -> String {
    match (entity_type, entity_id) {
        (Some("photo"), Some(id)) => format!("/photo{id}"),
        (Some("video"), Some(id)) => format!("/video{id}"),
        (Some("wall"), Some(id)) => format!("/wall{id}"),
        _ => "/notifications".into(),
    }
}
