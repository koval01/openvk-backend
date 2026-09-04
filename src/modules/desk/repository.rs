use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use sea_orm::{DatabaseTransaction, Value};

use crate::error::AppError;
use crate::ids::wall_permalink;
use crate::modules::desk::models::{
    AdminOverview, BannedLink, Gift, GiftCategory, NospamHit, NospamResult, Report, Ticket,
    TicketReply, UserGift, Voucher, Warning,
};

pub struct DeskRepository<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> DeskRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn gift_catalog(&self) -> Result<Vec<GiftCategory>, AppError> {
        let cats = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, slug, name, description FROM gift_categories ORDER BY sort, id",
            ))
            .await?;
        let gifts = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, category_id, name, description, price, image_url FROM gifts ORDER BY id",
            ))
            .await?;
        let mut catalog = Vec::new();
        for row in cats {
            let id: i64 = row.try_get_by_index(0)?;
            catalog.push(GiftCategory {
                id,
                slug: row.try_get_by_index(1)?,
                name: row.try_get_by_index(2)?,
                description: row.try_get_by_index(3)?,
                gifts: Vec::new(),
            });
        }
        for row in gifts {
            let gift = gift_from_row(&row)?;
            if let Some(cat) = catalog.iter_mut().find(|cat| cat.id == gift.category_id) {
                cat.gifts.push(gift);
            }
        }
        Ok(catalog)
    }

    pub async fn user_gifts(&self, receiver_id: i64) -> Result<Vec<UserGiftRow>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT ug.id, ug.gift_id, ug.sender_id, ug.receiver_id, ug.caption, ug.anonymous, ug.created_at,
                        g.category_id, g.name, g.description, g.price, g.image_url
                 FROM user_gifts ug
                 JOIN gifts g ON g.id = ug.gift_id
                 WHERE ug.receiver_id = $1
                 ORDER BY ug.id DESC
                 LIMIT 100",
                [receiver_id.into()],
            ))
            .await?;
        rows.into_iter().map(user_gift_row).collect()
    }

    pub async fn send_gift(
        &self,
        sender_id: i64,
        receiver_id: i64,
        gift_id: i64,
        caption: Option<String>,
        anonymous: bool,
    ) -> Result<UserGiftRow, AppError> {
        let txn = self.db.begin().await?;
        let gift = self
            .gift_in(&txn, gift_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let spent = self.debit_coins(&txn, sender_id, gift.price).await?;
        if !spent {
            txn.rollback().await?;
            return Err(AppError::Validation("not enough votes".into()));
        }
        let row = txn
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO user_gifts (gift_id, sender_id, receiver_id, caption, anonymous)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING id, gift_id, sender_id, receiver_id, caption, anonymous, created_at",
                [
                    gift_id.into(),
                    sender_id.into(),
                    receiver_id.into(),
                    caption.into(),
                    anonymous.into(),
                ],
            ))
            .await?
            .ok_or_else(|| AppError::internal("gift insert returned no row"))?;
        txn.commit().await?;
        Ok(UserGiftRow {
            id: row.try_get_by_index(0)?,
            gift,
            sender_id: row.try_get_by_index(2)?,
            receiver_id: row.try_get_by_index(3)?,
            caption: row.try_get_by_index(4)?,
            anonymous: row.try_get_by_index(5)?,
            created_at: row.try_get_by_index(6)?,
        })
    }

    pub async fn transfer_coins(
        &self,
        sender_id: i64,
        receiver_id: i64,
        amount: i64,
    ) -> Result<(), AppError> {
        let txn = self.db.begin().await?;
        if !self.debit_coins(&txn, sender_id, amount).await? {
            txn.rollback().await?;
            return Err(AppError::Validation("not enough votes".into()));
        }
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users SET coins = coins + $2 WHERE id = $1",
            [receiver_id.into(), amount.into()],
        ))
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn redeem_voucher(&self, user_id: i64, serial: &str) -> Result<i64, AppError> {
        let txn = self.db.begin().await?;
        let row = txn
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, coins, remaining, expires_at FROM vouchers WHERE serial = $1 FOR UPDATE",
                [serial.into()],
            ))
            .await?
            .ok_or_else(|| AppError::Validation("voucher not found".into()))?;
        let voucher_id: i64 = row.try_get_by_index(0)?;
        let coins: i64 = row.try_get_by_index(1)?;
        let remaining: i32 = row.try_get_by_index(2)?;
        let expires_at: Option<DateTime<Utc>> = row.try_get_by_index(3)?;
        if remaining <= 0 {
            txn.rollback().await?;
            return Err(AppError::Validation("voucher has no uses left".into()));
        }
        if expires_at.is_some_and(|until| until <= Utc::now()) {
            txn.rollback().await?;
            return Err(AppError::Validation("voucher has expired".into()));
        }
        let already = txn
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT 1 FROM voucher_redemptions WHERE voucher_id = $1 AND user_id = $2",
                [voucher_id.into(), user_id.into()],
            ))
            .await?;
        if already.is_some() {
            txn.rollback().await?;
            return Err(AppError::Validation("voucher already redeemed".into()));
        }
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO voucher_redemptions (voucher_id, user_id) VALUES ($1, $2)",
            [voucher_id.into(), user_id.into()],
        ))
        .await?;
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE vouchers SET remaining = remaining - 1 WHERE id = $1",
            [voucher_id.into()],
        ))
        .await?;
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users SET coins = coins + $2 WHERE id = $1",
            [user_id.into(), coins.into()],
        ))
        .await?;
        txn.commit().await?;
        Ok(coins)
    }

    pub async fn list_vouchers(&self) -> Result<Vec<Voucher>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, serial, coins, remaining, total, expires_at FROM vouchers ORDER BY id DESC LIMIT 100",
            ))
            .await?;
        rows.into_iter().map(voucher_from_row).collect()
    }

    pub async fn get_voucher(&self, id: i64) -> Result<Option<Voucher>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, serial, coins, remaining, total, expires_at FROM vouchers WHERE id = $1",
                [id.into()],
            ))
            .await?;
        row.map(voucher_from_row).transpose()
    }

    pub async fn create_voucher(
        &self,
        serial: &str,
        coins: i64,
        uses: i32,
    ) -> Result<Voucher, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO vouchers (serial, coins, remaining, total)
                 VALUES ($1, $2, $3, $3)
                 RETURNING id, serial, coins, remaining, total, expires_at",
                [serial.into(), coins.into(), uses.into()],
            ))
            .await?
            .ok_or_else(|| AppError::internal("voucher insert returned no row"))?;
        voucher_from_row(row)
    }

    pub async fn create_ticket(
        &self,
        author_id: i64,
        subject: &str,
        content: &str,
    ) -> Result<i64, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO tickets (author_id, subject, content)
                 VALUES ($1, $2, $3) RETURNING id",
                [author_id.into(), subject.into(), content.into()],
            ))
            .await?
            .ok_or_else(|| AppError::internal("ticket insert returned no row"))?;
        Ok(row.try_get_by_index(0)?)
    }

    pub async fn list_tickets(&self, author_id: Option<i64>) -> Result<Vec<TicketRow>, AppError> {
        let (sql, values): (&str, Vec<Value>) = if let Some(author_id) = author_id {
            (
                "SELECT id, author_id, subject, content, status, created_at
                 FROM tickets WHERE author_id = $1 ORDER BY id DESC LIMIT 100",
                vec![author_id.into()],
            )
        } else {
            (
                "SELECT id, author_id, subject, content, status, created_at
                 FROM tickets ORDER BY id DESC LIMIT 100",
                Vec::new(),
            )
        };
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                values,
            ))
            .await?;
        rows.into_iter().map(ticket_row).collect()
    }

    pub async fn get_ticket(&self, id: i64) -> Result<Option<TicketRow>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, author_id, subject, content, status, created_at FROM tickets WHERE id = $1",
                [id.into()],
            ))
            .await?;
        row.map(ticket_row).transpose()
    }

    pub async fn ticket_replies(&self, ticket_id: i64) -> Result<Vec<ReplyRow>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, ticket_id, author_id, content, from_agent, created_at
                 FROM ticket_replies WHERE ticket_id = $1 ORDER BY id",
                [ticket_id.into()],
            ))
            .await?;
        rows.into_iter().map(reply_row).collect()
    }

    pub async fn add_ticket_reply(
        &self,
        ticket_id: i64,
        author_id: i64,
        content: &str,
        from_agent: bool,
    ) -> Result<i64, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO ticket_replies (ticket_id, author_id, content, from_agent)
                 VALUES ($1, $2, $3, $4) RETURNING id",
                [
                    ticket_id.into(),
                    author_id.into(),
                    content.into(),
                    from_agent.into(),
                ],
            ))
            .await?
            .ok_or_else(|| AppError::internal("reply insert returned no row"))?;
        Ok(row.try_get_by_index(0)?)
    }

    pub async fn set_ticket_status(&self, id: i64, status: &str) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE tickets SET status = $2 WHERE id = $1",
                [id.into(), status.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn delete_ticket(&self, id: i64) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "DELETE FROM tickets WHERE id = $1",
                [id.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn create_report(
        &self,
        author_id: i64,
        target_type: &str,
        target_id: i64,
        reason: &str,
    ) -> Result<i64, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO reports (author_id, target_type, target_id, reason)
                 VALUES ($1, $2, $3, $4) RETURNING id",
                [
                    author_id.into(),
                    target_type.into(),
                    target_id.into(),
                    reason.into(),
                ],
            ))
            .await?
            .ok_or_else(|| AppError::internal("report insert returned no row"))?;
        Ok(row.try_get_by_index(0)?)
    }

    pub async fn list_reports(&self) -> Result<Vec<ReportRow>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, author_id, target_type, target_id, reason, status, created_at
                 FROM reports ORDER BY id DESC LIMIT 100",
            ))
            .await?;
        rows.into_iter().map(report_row).collect()
    }

    pub async fn get_report(&self, id: i64) -> Result<Option<ReportRow>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, author_id, target_type, target_id, reason, status, created_at
                 FROM reports WHERE id = $1",
                [id.into()],
            ))
            .await?;
        row.map(report_row).transpose()
    }

    pub async fn set_report_status(&self, id: i64, status: &str) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE reports SET status = $2 WHERE id = $1",
                [id.into(), status.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn ban_user(
        &self,
        user_id: i64,
        until: Option<DateTime<Utc>>,
        reason: &str,
    ) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET banned = TRUE, banned_until = $2, ban_reason = $3 WHERE id = $1",
                [user_id.into(), until.into(), reason.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn unban_user(&self, user_id: i64) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET banned = FALSE, banned_until = NULL, ban_reason = NULL WHERE id = $1",
                [user_id.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn support_ban(&self, user_id: i64, reason: &str) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET support_banned = TRUE, support_ban_reason = $2 WHERE id = $1",
                [user_id.into(), reason.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn support_unban(&self, user_id: i64) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET support_banned = FALSE, support_ban_reason = NULL WHERE id = $1",
                [user_id.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn set_limits(
        &self,
        user_id: i64,
        posting_allowed: bool,
        messaging_allowed: bool,
    ) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET posting_allowed = $2, messaging_allowed = $3 WHERE id = $1",
                [
                    user_id.into(),
                    posting_allowed.into(),
                    messaging_allowed.into(),
                ],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn add_warning(
        &self,
        user_id: i64,
        actor_id: i64,
        reason: &str,
    ) -> Result<i64, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO warnings (user_id, actor_id, reason) VALUES ($1, $2, $3) RETURNING id",
                [user_id.into(), actor_id.into(), reason.into()],
            ))
            .await?
            .ok_or_else(|| AppError::internal("warning insert returned no row"))?;
        Ok(row.try_get_by_index(0)?)
    }

    pub async fn list_warnings(&self, user_id: i64) -> Result<Vec<Warning>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, user_id, actor_id, reason, created_at
                 FROM warnings WHERE user_id = $1 ORDER BY id DESC",
                [user_id.into()],
            ))
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(Warning {
                    id: row.try_get_by_index(0)?,
                    user_id: row.try_get_by_index(1)?,
                    actor_id: row.try_get_by_index(2)?,
                    reason: row.try_get_by_index(3)?,
                    created_at: row.try_get_by_index(4)?,
                })
            })
            .collect()
    }

    pub async fn list_banned_links(&self) -> Result<Vec<BannedLink>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, url, reason, created_at FROM banned_links ORDER BY id DESC LIMIT 200",
            ))
            .await?;
        rows.into_iter().map(banned_link_from_row).collect()
    }

    pub async fn get_banned_link(&self, id: i64) -> Result<Option<BannedLink>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, url, reason, created_at FROM banned_links WHERE id = $1",
                [id.into()],
            ))
            .await?;
        row.map(banned_link_from_row).transpose()
    }

    pub async fn find_banned_link(&self, url: &str) -> Result<Option<BannedLink>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, url, reason, created_at FROM banned_links
                 WHERE lower($1) LIKE '%' || lower(url) || '%'
                    OR lower(url) = lower($1)
                 ORDER BY id
                 LIMIT 1",
                [url.into()],
            ))
            .await?;
        row.map(banned_link_from_row).transpose()
    }

    pub async fn add_banned_link(&self, url: &str, reason: &str) -> Result<BannedLink, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO banned_links (url, reason) VALUES ($1, $2)
                 RETURNING id, url, reason, created_at",
                [url.into(), reason.into()],
            ))
            .await?
            .ok_or_else(|| AppError::internal("banned link insert returned no row"))?;
        banned_link_from_row(row)
    }

    pub async fn delete_banned_link(&self, id: i64) -> Result<(), AppError> {
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "DELETE FROM banned_links WHERE id = $1",
                [id.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn find_wall_post_id(
        &self,
        owner_id: i64,
        local_id: i64,
    ) -> Result<Option<i64>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id FROM wall_posts
                 WHERE target_id = $1 AND local_id = $2 AND deleted_at IS NULL",
                [owner_id.into(), local_id.into()],
            ))
            .await?;
        Ok(match row {
            Some(row) => Some(row.try_get_by_index(0)?),
            None => None,
        })
    }

    pub async fn search_wall(&self, query: &str) -> Result<Vec<NospamHit>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, target_id, local_id, author_id, content
                 FROM wall_posts
                 WHERE deleted_at IS NULL AND content ILIKE '%' || $1 || '%'
                 ORDER BY id DESC
                 LIMIT 50",
                [query.into()],
            ))
            .await?;
        rows.into_iter()
            .map(|row| {
                let target_id: i64 = row.try_get_by_index(1)?;
                let local_id: i64 = row.try_get_by_index(2)?;
                Ok(NospamHit {
                    post_id: row.try_get_by_index(0)?,
                    target_id,
                    local_id,
                    author_id: row.try_get_by_index(3)?,
                    content: row.try_get_by_index(4)?,
                    permalink: wall_permalink(target_id, local_id),
                })
            })
            .collect()
    }

    pub async fn delete_posts(&self, ids: &[i64]) -> Result<i32, AppError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let result = self
            .db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE wall_posts SET deleted_at = now()
                 WHERE id = ANY($1::bigint[]) AND deleted_at IS NULL",
                [ids_sql(ids)],
            ))
            .await?;
        Ok(i32::try_from(result.rows_affected()).unwrap_or(i32::MAX))
    }

    pub async fn restore_posts(&self, ids: &[i64]) -> Result<(), AppError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE wall_posts SET deleted_at = NULL WHERE id = ANY($1::bigint[])",
                [ids_sql(ids)],
            ))
            .await?;
        Ok(())
    }

    pub async fn save_nospam_action(
        &self,
        actor_id: i64,
        query: &str,
        ids: &[i64],
    ) -> Result<i64, AppError> {
        let packed = ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO nospam_actions (actor_id, query, post_ids)
                 VALUES ($1, $2, $3) RETURNING id",
                [actor_id.into(), query.into(), packed.into()],
            ))
            .await?
            .ok_or_else(|| AppError::internal("nospam insert returned no row"))?;
        Ok(row.try_get_by_index(0)?)
    }

    pub async fn nospam_action_ids(&self, id: i64) -> Result<Option<Vec<i64>>, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT post_ids FROM nospam_actions WHERE id = $1",
                [id.into()],
            ))
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let packed: String = row.try_get_by_index(0)?;
        let ids = packed
            .split(',')
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.parse::<i64>()
                    .map_err(|_| AppError::internal("invalid nospam post id"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(ids))
    }

    pub async fn overview(&self) -> Result<AdminOverview, AppError> {
        let row = self
            .db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT
                    (SELECT COUNT(*) FROM users) AS users,
                    (SELECT COUNT(*) FROM groups) AS groups,
                    (SELECT COUNT(*) FROM wall_posts WHERE deleted_at IS NULL) AS posts,
                    (SELECT COUNT(*) FROM tickets WHERE status = 'open') AS tickets,
                    (SELECT COUNT(*) FROM reports WHERE status = 'open') AS reports,
                    (SELECT COUNT(*) FROM users WHERE banned) AS banned",
            ))
            .await?
            .ok_or_else(|| AppError::internal("overview query returned no row"))?;
        Ok(AdminOverview {
            users: row.try_get_by_index(0)?,
            groups: row.try_get_by_index(1)?,
            wall_posts: row.try_get_by_index(2)?,
            tickets_open: row.try_get_by_index(3)?,
            reports_open: row.try_get_by_index(4)?,
            banned_users: row.try_get_by_index(5)?,
        })
    }

    pub async fn search_user_ids(&self, query: &str) -> Result<Vec<i64>, AppError> {
        let like = format!("%{query}%");
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id FROM users
                 WHERE login ILIKE $1 OR first_name ILIKE $1 OR last_name ILIKE $1
                    OR COALESCE(screen_name, '') ILIKE $1
                 ORDER BY id
                 LIMIT 50",
                [like.into()],
            ))
            .await?;
        rows.into_iter()
            .map(|row| Ok(row.try_get_by_index(0)?))
            .collect()
    }

    pub async fn list_user_ids(&self) -> Result<Vec<i64>, AppError> {
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id FROM users ORDER BY created_at DESC LIMIT 50",
            ))
            .await?;
        rows.into_iter()
            .map(|row| Ok(row.try_get_by_index(0)?))
            .collect()
    }

    async fn gift_in(&self, txn: &DatabaseTransaction, id: i64) -> Result<Option<Gift>, AppError> {
        let row = txn
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id, category_id, name, description, price, image_url FROM gifts WHERE id = $1",
                [id.into()],
            ))
            .await?;
        row.map(|row| gift_from_row(&row)).transpose()
    }

    async fn debit_coins(
        &self,
        txn: &DatabaseTransaction,
        user_id: i64,
        amount: i64,
    ) -> Result<bool, AppError> {
        let result = txn
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE users SET coins = coins - $2 WHERE id = $1 AND coins >= $2",
                [user_id.into(), amount.into()],
            ))
            .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Clone, Debug)]
pub struct UserGiftRow {
    pub id: i64,
    pub gift: Gift,
    pub sender_id: i64,
    pub receiver_id: i64,
    pub caption: Option<String>,
    pub anonymous: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct TicketRow {
    pub id: i64,
    pub author_id: i64,
    pub subject: String,
    pub content: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ReplyRow {
    pub id: i64,
    pub ticket_id: i64,
    pub author_id: i64,
    pub content: String,
    pub from_agent: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ReportRow {
    pub id: i64,
    pub author_id: i64,
    pub target_type: String,
    pub target_id: i64,
    pub reason: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

fn gift_from_row(row: &sea_orm::QueryResult) -> Result<Gift, AppError> {
    Ok(Gift {
        id: row.try_get_by_index(0)?,
        category_id: row.try_get_by_index(1)?,
        name: row.try_get_by_index(2)?,
        description: row.try_get_by_index(3)?,
        price: row.try_get_by_index(4)?,
        image_url: row.try_get_by_index(5)?,
    })
}

fn user_gift_row(row: sea_orm::QueryResult) -> Result<UserGiftRow, AppError> {
    Ok(UserGiftRow {
        id: row.try_get_by_index(0)?,
        gift: Gift {
            id: row.try_get_by_index(1)?,
            category_id: row.try_get_by_index(7)?,
            name: row.try_get_by_index(8)?,
            description: row.try_get_by_index(9)?,
            price: row.try_get_by_index(10)?,
            image_url: row.try_get_by_index(11)?,
        },
        sender_id: row.try_get_by_index(2)?,
        receiver_id: row.try_get_by_index(3)?,
        caption: row.try_get_by_index(4)?,
        anonymous: row.try_get_by_index(5)?,
        created_at: row.try_get_by_index(6)?,
    })
}

fn voucher_from_row(row: sea_orm::QueryResult) -> Result<Voucher, AppError> {
    Ok(Voucher {
        id: row.try_get_by_index(0)?,
        serial: row.try_get_by_index(1)?,
        coins: row.try_get_by_index(2)?,
        remaining: row.try_get_by_index(3)?,
        total: row.try_get_by_index(4)?,
        expires_at: row.try_get_by_index(5)?,
    })
}

fn ticket_row(row: sea_orm::QueryResult) -> Result<TicketRow, AppError> {
    Ok(TicketRow {
        id: row.try_get_by_index(0)?,
        author_id: row.try_get_by_index(1)?,
        subject: row.try_get_by_index(2)?,
        content: row.try_get_by_index(3)?,
        status: row.try_get_by_index(4)?,
        created_at: row.try_get_by_index(5)?,
    })
}

fn reply_row(row: sea_orm::QueryResult) -> Result<ReplyRow, AppError> {
    Ok(ReplyRow {
        id: row.try_get_by_index(0)?,
        ticket_id: row.try_get_by_index(1)?,
        author_id: row.try_get_by_index(2)?,
        content: row.try_get_by_index(3)?,
        from_agent: row.try_get_by_index(4)?,
        created_at: row.try_get_by_index(5)?,
    })
}

fn report_row(row: sea_orm::QueryResult) -> Result<ReportRow, AppError> {
    Ok(ReportRow {
        id: row.try_get_by_index(0)?,
        author_id: row.try_get_by_index(1)?,
        target_type: row.try_get_by_index(2)?,
        target_id: row.try_get_by_index(3)?,
        reason: row.try_get_by_index(4)?,
        status: row.try_get_by_index(5)?,
        created_at: row.try_get_by_index(6)?,
    })
}

fn banned_link_from_row(row: sea_orm::QueryResult) -> Result<BannedLink, AppError> {
    Ok(BannedLink {
        id: row.try_get_by_index(0)?,
        url: row.try_get_by_index(1)?,
        reason: row.try_get_by_index(2)?,
        created_at: row.try_get_by_index(3)?,
    })
}

fn ids_sql(ids: &[i64]) -> Value {
    let literal = ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{literal}}}").into()
}

#[allow(dead_code)]
pub fn into_user_gift(row: UserGiftRow) -> UserGift {
    UserGift {
        id: row.id,
        gift_id: row.gift.id,
        gift: row.gift,
        sender_id: row.sender_id,
        sender: None,
        receiver_id: row.receiver_id,
        caption: row.caption,
        anonymous: row.anonymous,
        created_at: row.created_at,
    }
}

#[allow(dead_code)]
pub fn into_ticket(row: TicketRow) -> Ticket {
    Ticket {
        id: row.id,
        author_id: row.author_id,
        author: dummy_user(row.author_id),
        subject: row.subject,
        content: row.content,
        status: row.status,
        created_at: row.created_at,
        replies: Vec::new(),
    }
}

#[allow(dead_code)]
pub fn into_reply(row: ReplyRow) -> TicketReply {
    TicketReply {
        id: row.id,
        ticket_id: row.ticket_id,
        author_id: row.author_id,
        author: dummy_user(row.author_id),
        content: row.content,
        from_agent: row.from_agent,
        created_at: row.created_at,
    }
}

#[allow(dead_code)]
pub fn into_report(row: ReportRow) -> Report {
    Report {
        id: row.id,
        author_id: row.author_id,
        author: dummy_user(row.author_id),
        target_type: row.target_type,
        target_id: row.target_id,
        reason: row.reason,
        status: row.status,
        created_at: row.created_at,
    }
}

fn dummy_user(id: i64) -> crate::modules::users::User {
    crate::modules::users::User {
        id,
        first_name: String::new(),
        last_name: String::new(),
        screen_name: None,
        status: None,
        city: None,
        email: None,
        phone: None,
        avatar_url: None,
        verified: false,
        privacy_wall: crate::modules::users::PrivacyLevel::Everyone,
        privacy_messages: crate::modules::users::PrivacyLevel::Everyone,
        privacy_photos: crate::modules::users::PrivacyLevel::Everyone,
        privacy_audio: crate::modules::users::PrivacyLevel::Everyone,
        privacy_profile: crate::modules::users::PrivacyLevel::Everyone,
        privacy_friends: crate::modules::users::PrivacyLevel::Everyone,
        created_at: Utc::now(),
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

#[allow(dead_code)]
pub fn nospam_result(action_id: i64, hits: Vec<NospamHit>, deleted: i32) -> NospamResult {
    NospamResult {
        action_id,
        hits,
        deleted,
    }
}
