use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
    TransactionTrait,
};

use crate::db::entities::user::Entity as UserEntity;
use crate::db::entities::{conversation, conversation_key, conversation_member, message, user};
use crate::error::AppError;
use crate::ids::{is_id_collision, random_public_id};
use crate::modules::messenger::models::Message;
use crate::vault::Vault;

pub struct MessageRepository<'a> {
    db: &'a DatabaseConnection,
    vault: &'a Vault,
}

impl<'a> MessageRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection, vault: &'a Vault) -> Self {
        Self { db, vault }
    }

    pub async fn list_thread(&self, user_id: i64, peer_id: i64) -> Result<Vec<Message>, AppError> {
        let Some(conversation_id) = self.find_direct(user_id, peer_id).await? else {
            return Ok(Vec::new());
        };
        let dek = self.conversation_dek(conversation_id, user_id).await?;
        let rows = message::Entity::find()
            .filter(message::Column::ConversationId.eq(conversation_id))
            .filter(message::Column::DeletedAt.is_null())
            .order_by_asc(message::Column::CreatedAt)
            .limit(100)
            .all(self.db)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(Message {
                    id: row.id,
                    peer_id,
                    author_id: row.author_id,
                    text: Vault::decrypt_message(&dek, conversation_id, &row.content)?,
                    created_at: row.created_at,
                })
            })
            .collect()
    }

    pub async fn list_inbox(&self, user_id: i64) -> Result<Vec<Message>, AppError> {
        let sql = "
            SELECT
                c.id,
                peer.user_id,
                m.id,
                m.author_id,
                m.content,
                m.created_at
            FROM conversations c
            JOIN conversation_members me
                ON me.conversation_id = c.id AND me.user_id = $1
            JOIN conversation_members peer
                ON peer.conversation_id = c.id AND peer.user_id <> $1
            JOIN LATERAL (
                SELECT id, author_id, content, created_at
                FROM messages
                WHERE conversation_id = c.id AND deleted_at IS NULL
                ORDER BY created_at DESC
                LIMIT 1
            ) m ON true
            WHERE c.kind = 'direct'
            ORDER BY m.created_at DESC
            LIMIT 50
        ";
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                [user_id.into()],
            ))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let conversation_id: i64 = row.try_get_by_index(0).map_err(sea_orm::DbErr::from)?;
            let peer_id: i64 = row.try_get_by_index(1).map_err(sea_orm::DbErr::from)?;
            let id: i64 = row.try_get_by_index(2).map_err(sea_orm::DbErr::from)?;
            let author_id: i64 = row.try_get_by_index(3).map_err(sea_orm::DbErr::from)?;
            let content: String = row.try_get_by_index(4).map_err(sea_orm::DbErr::from)?;
            let created_at: chrono::DateTime<Utc> =
                row.try_get_by_index(5).map_err(sea_orm::DbErr::from)?;
            let dek = self.conversation_dek(conversation_id, user_id).await?;
            out.push(Message {
                id,
                peer_id,
                author_id,
                text: Vault::decrypt_message(&dek, conversation_id, &content)?,
                created_at,
            });
        }
        Ok(out)
    }

    pub async fn send(
        &self,
        author_id: i64,
        peer_id: i64,
        text: &str,
    ) -> Result<Message, AppError> {
        if UserEntity::find_by_id(peer_id)
            .one(self.db)
            .await?
            .is_none()
        {
            return Err(AppError::NotFound);
        }
        let conversation_id = self.find_or_create_direct(author_id, peer_id).await?;
        let dek = self.conversation_dek(conversation_id, author_id).await?;
        let content = Vault::encrypt_message(&dek, conversation_id, text)?;
        let row = message::ActiveModel {
            conversation_id: Set(conversation_id),
            author_id: Set(author_id),
            content: Set(content),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(self.db)
        .await?;
        Ok(Message {
            id: row.id,
            peer_id,
            author_id: row.author_id,
            text: text.to_owned(),
            created_at: row.created_at,
        })
    }

    async fn find_or_create_direct(&self, user_id: i64, peer_id: i64) -> Result<i64, AppError> {
        if let Some(existing) = self.find_direct(user_id, peer_id).await? {
            return Ok(existing);
        }

        for _ in 0..32 {
            let txn = self.db.begin().await?;
            let conversation_id = random_public_id();
            let now = Utc::now();
            let model = conversation::ActiveModel {
                id: Set(conversation_id),
                kind: Set("direct".into()),
                title: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            };
            match model.insert(&txn).await {
                Ok(_) => {}
                Err(error) if is_id_collision(&error) => {
                    txn.rollback().await?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            }

            let members = unique_members(user_id, peer_id);
            for member_id in &members {
                conversation_member::ActiveModel {
                    conversation_id: Set(conversation_id),
                    user_id: Set(*member_id),
                    role: Set("member".into()),
                    joined_at: Set(now),
                    ..Default::default()
                }
                .insert(&txn)
                .await?;
            }

            let dek = Vault::random_key();
            for member_id in members {
                let wrap = self.user_wrap_key_in(&txn, member_id).await?;
                let wrapped =
                    Vault::wrap_conversation_dek(&wrap, conversation_id, member_id, &dek)?;
                conversation_key::ActiveModel {
                    conversation_id: Set(conversation_id),
                    user_id: Set(member_id),
                    wrapped_dek: Set(wrapped),
                    created_at: Set(now),
                }
                .insert(&txn)
                .await?;
            }

            txn.commit().await?;
            return Ok(conversation_id);
        }

        Err(AppError::internal("could not allocate a conversation id"))
    }

    async fn find_direct(&self, user_id: i64, peer_id: i64) -> Result<Option<i64>, AppError> {
        let sql = if user_id == peer_id {
            "
            SELECT c.id
            FROM conversations c
            JOIN conversation_members a
                ON a.conversation_id = c.id AND a.user_id = $1
            WHERE c.kind = 'direct'
              AND NOT EXISTS (
                  SELECT 1
                  FROM conversation_members other
                  WHERE other.conversation_id = c.id AND other.user_id <> $1
              )
            LIMIT 1
            "
        } else {
            "
            SELECT c.id
            FROM conversations c
            JOIN conversation_members a
                ON a.conversation_id = c.id AND a.user_id = $1
            JOIN conversation_members b
                ON b.conversation_id = c.id AND b.user_id = $2
            WHERE c.kind = 'direct'
              AND (
                  SELECT COUNT(*)
                  FROM conversation_members m
                  WHERE m.conversation_id = c.id
              ) = 2
            LIMIT 1
            "
        };
        let values = if user_id == peer_id {
            vec![user_id.into()]
        } else {
            vec![user_id.into(), peer_id.into()]
        };
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                sql,
                values,
            ))
            .await?;
        Ok(row.and_then(|row| row.try_get_by_index::<i64>(0).ok()))
    }

    async fn conversation_dek(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<[u8; 32], AppError> {
        let row = conversation_key::Entity::find_by_id((conversation_id, user_id))
            .one(self.db)
            .await?
            .ok_or(AppError::Forbidden)?;
        let wrap = self.user_wrap_key(user_id).await?;
        Vault::unwrap_conversation_dek(&wrap, conversation_id, user_id, &row.wrapped_dek)
    }

    async fn user_wrap_key(&self, user_id: i64) -> Result<[u8; 32], AppError> {
        self.user_wrap_key_in(self.db, user_id).await
    }

    async fn user_wrap_key_in<C: ConnectionTrait>(
        &self,
        db: &C,
        user_id: i64,
    ) -> Result<[u8; 32], AppError> {
        let model = UserEntity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound)?;
        if let Some(stored) = &model.wrap_key {
            return self.vault.unwrap_user_key(user_id, stored);
        }
        let key = Vault::random_key();
        let mut active: user::ActiveModel = model.into();
        active.wrap_key = Set(Some(self.vault.wrap_user_key(user_id, &key)?));
        active.update(db).await?;
        Ok(key)
    }
}

fn unique_members(user_id: i64, peer_id: i64) -> Vec<i64> {
    if user_id == peer_id {
        vec![user_id]
    } else {
        vec![user_id, peer_id]
    }
}
