use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rand::Rng;

use crate::error::AppError;
use crate::modules::desk::models::{
    AdminOverview, BannedLink, GiftCategory, NospamResult, Report, Ticket, TicketReply, UserGift,
    Voucher, Warning,
};
use crate::modules::desk::repository::{self, DeskRepository};
use crate::modules::groups::models::Group;
use crate::modules::users::User;
use crate::modules::users::services as users;
use crate::pb;
use crate::state::AppState;

pub async fn require_admin(state: &AppState, user_id: i64) -> Result<User, AppError> {
    let user = users::get_profile(state, user_id).await?;
    if user.is_admin() {
        Ok(user)
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn require_agent(state: &AppState, user_id: i64) -> Result<User, AppError> {
    let user = users::get_profile(state, user_id).await?;
    if user.is_agent() {
        Ok(user)
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn catalog(state: &AppState) -> Result<Vec<GiftCategory>, AppError> {
    DeskRepository::new(&state.db).gift_catalog().await
}

pub async fn user_gifts(state: &AppState, receiver_id: i64) -> Result<Vec<UserGift>, AppError> {
    let rows = DeskRepository::new(&state.db)
        .user_gifts(receiver_id)
        .await?;
    let ids: Vec<i64> = rows
        .iter()
        .filter(|row| !row.anonymous)
        .map(|row| row.sender_id)
        .collect();
    let map = load_users(state, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let sender = if row.anonymous {
                None
            } else {
                map.get(&row.sender_id).cloned()
            };
            let mut gift = repository::into_user_gift(row);
            gift.sender = sender;
            gift
        })
        .collect())
}

pub async fn send_gift(
    state: &AppState,
    sender_id: i64,
    body: pb::SendGift,
) -> Result<UserGift, AppError> {
    if body.receiver_id <= 0 || body.receiver_id == sender_id {
        return Err(AppError::Validation("choose another person".into()));
    }
    users::get_profile(state, body.receiver_id).await?;
    let caption = body
        .caption
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if caption
        .as_ref()
        .is_some_and(|value| value.chars().count() > 400)
    {
        return Err(AppError::Validation("caption is too long".into()));
    }
    let row = DeskRepository::new(&state.db)
        .send_gift(
            sender_id,
            body.receiver_id,
            body.gift_id,
            caption,
            body.anonymous,
        )
        .await?;
    users::reload(state, sender_id).await?;
    let sender = if row.anonymous {
        None
    } else {
        Some(users::get_profile(state, sender_id).await?)
    };
    let mut gift = repository::into_user_gift(row);
    gift.sender = sender;
    Ok(gift)
}

pub async fn transfer_coins(
    state: &AppState,
    sender_id: i64,
    body: pb::TransferCoins,
) -> Result<User, AppError> {
    if body.amount <= 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if body.receiver_id <= 0 || body.receiver_id == sender_id {
        return Err(AppError::Validation("choose another person".into()));
    }
    users::get_profile(state, body.receiver_id).await?;
    DeskRepository::new(&state.db)
        .transfer_coins(sender_id, body.receiver_id, body.amount)
        .await?;
    users::reload(state, body.receiver_id).await?;
    users::reload(state, sender_id).await
}

pub async fn redeem_voucher(
    state: &AppState,
    user_id: i64,
    serial: &str,
) -> Result<User, AppError> {
    let serial = normalize_serial(serial)?;
    DeskRepository::new(&state.db)
        .redeem_voucher(user_id, &serial)
        .await?;
    users::reload(state, user_id).await
}

pub async fn list_vouchers(state: &AppState, actor_id: i64) -> Result<Vec<Voucher>, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).list_vouchers().await
}

pub async fn get_voucher(state: &AppState, actor_id: i64, id: i64) -> Result<Voucher, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db)
        .get_voucher(id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn create_voucher(
    state: &AppState,
    actor_id: i64,
    coins: i64,
    uses: i32,
) -> Result<Voucher, AppError> {
    require_admin(state, actor_id).await?;
    if coins <= 0 || uses <= 0 {
        return Err(AppError::Validation(
            "voucher coins and uses must be positive".into(),
        ));
    }
    DeskRepository::new(&state.db)
        .create_voucher(&random_serial(), coins, uses)
        .await
}

pub async fn create_ticket(
    state: &AppState,
    author_id: i64,
    body: pb::WriteTicket,
) -> Result<Ticket, AppError> {
    let author = users::get_profile(state, author_id).await?;
    if author.support_banned {
        return Err(AppError::Forbidden);
    }
    let subject = body.subject.trim().to_owned();
    let content = body.content.trim().to_owned();
    if subject.is_empty() || content.is_empty() {
        return Err(AppError::Validation("subject and text are required".into()));
    }
    if subject.chars().count() > 200 || content.chars().count() > 8000 {
        return Err(AppError::Validation("ticket is too long".into()));
    }
    let id = DeskRepository::new(&state.db)
        .create_ticket(author_id, &subject, &content)
        .await?;
    get_ticket(state, author_id, id).await
}

pub async fn list_tickets(
    state: &AppState,
    user_id: i64,
    all: bool,
) -> Result<Vec<Ticket>, AppError> {
    let actor = users::get_profile(state, user_id).await?;
    let author_id = if all {
        if !actor.is_agent() {
            return Err(AppError::Forbidden);
        }
        None
    } else {
        Some(user_id)
    };
    let rows = DeskRepository::new(&state.db)
        .list_tickets(author_id)
        .await?;
    hydrate_tickets(state, rows, false).await
}

pub async fn get_ticket(state: &AppState, user_id: i64, id: i64) -> Result<Ticket, AppError> {
    let actor = users::get_profile(state, user_id).await?;
    let row = DeskRepository::new(&state.db)
        .get_ticket(id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.author_id != user_id && !actor.is_agent() {
        return Err(AppError::Forbidden);
    }
    let mut tickets = hydrate_tickets(state, vec![row], true).await?;
    tickets.pop().ok_or(AppError::NotFound)
}

pub async fn reply_ticket(
    state: &AppState,
    user_id: i64,
    ticket_id: i64,
    content: &str,
) -> Result<Ticket, AppError> {
    let actor = users::get_profile(state, user_id).await?;
    let row = DeskRepository::new(&state.db)
        .get_ticket(ticket_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.author_id != user_id && !actor.is_agent() {
        return Err(AppError::Forbidden);
    }
    if row.status != "open" {
        return Err(AppError::Validation("ticket is closed".into()));
    }
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::Validation("reply cannot be empty".into()));
    }
    DeskRepository::new(&state.db)
        .add_ticket_reply(ticket_id, user_id, content, actor.is_agent())
        .await?;
    get_ticket(state, user_id, ticket_id).await
}

pub async fn close_ticket(
    state: &AppState,
    user_id: i64,
    ticket_id: i64,
) -> Result<Ticket, AppError> {
    let actor = users::get_profile(state, user_id).await?;
    let row = DeskRepository::new(&state.db)
        .get_ticket(ticket_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.author_id != user_id && !actor.is_agent() {
        return Err(AppError::Forbidden);
    }
    DeskRepository::new(&state.db)
        .set_ticket_status(ticket_id, "closed")
        .await?;
    get_ticket(state, user_id, ticket_id).await
}

pub async fn delete_ticket(state: &AppState, user_id: i64, ticket_id: i64) -> Result<(), AppError> {
    require_agent(state, user_id).await?;
    DeskRepository::new(&state.db)
        .delete_ticket(ticket_id)
        .await
}

pub async fn create_report(
    state: &AppState,
    author_id: i64,
    body: pb::WriteReport,
) -> Result<Report, AppError> {
    let target_type = body.target_type.trim().to_ascii_lowercase();
    if !matches!(
        target_type.as_str(),
        "user" | "wall" | "photo" | "video" | "club" | "comment"
    ) {
        return Err(AppError::Validation("unknown report target".into()));
    }
    let reason = body.reason.trim().to_owned();
    if reason.is_empty() {
        return Err(AppError::Validation("reason is required".into()));
    }
    let mut target_id = body.target_id;
    if target_type == "wall" {
        if let Some(owner_id) = body.owner_id.filter(|id| *id != 0) {
            target_id = DeskRepository::new(&state.db)
                .find_wall_post_id(owner_id, body.target_id)
                .await?
                .ok_or(AppError::NotFound)?;
        }
    }
    let id = DeskRepository::new(&state.db)
        .create_report(author_id, &target_type, target_id, &reason)
        .await?;
    let row = DeskRepository::new(&state.db)
        .get_report(id)
        .await?
        .ok_or(AppError::NotFound)?;
    hydrate_report(state, row).await
}

pub async fn list_reports(state: &AppState, actor_id: i64) -> Result<Vec<Report>, AppError> {
    require_agent(state, actor_id).await?;
    let rows = DeskRepository::new(&state.db).list_reports().await?;
    let mut reports = Vec::with_capacity(rows.len());
    for row in rows {
        reports.push(hydrate_report(state, row).await?);
    }
    Ok(reports)
}

pub async fn get_report(state: &AppState, actor_id: i64, id: i64) -> Result<Report, AppError> {
    require_agent(state, actor_id).await?;
    let row = DeskRepository::new(&state.db)
        .get_report(id)
        .await?
        .ok_or(AppError::NotFound)?;
    hydrate_report(state, row).await
}

pub async fn report_action(
    state: &AppState,
    actor_id: i64,
    id: i64,
    action: &str,
    reason: Option<&str>,
) -> Result<Report, AppError> {
    require_admin(state, actor_id).await?;
    let row = DeskRepository::new(&state.db)
        .get_report(id)
        .await?
        .ok_or(AppError::NotFound)?;
    match action {
        "ignore" | "close" => {
            DeskRepository::new(&state.db)
                .set_report_status(id, "closed")
                .await?;
        }
        "delete" => {
            if row.target_type == "wall" {
                DeskRepository::new(&state.db)
                    .delete_posts(&[row.target_id])
                    .await?;
            }
            DeskRepository::new(&state.db)
                .set_report_status(id, "deleted")
                .await?;
        }
        "ban" => {
            let target_user = match row.target_type.as_str() {
                "user" => row.target_id,
                "wall" => wall_author(state, row.target_id)
                    .await
                    .unwrap_or(row.target_id),
                _ => row.author_id,
            };
            let reason = reason
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("report");
            DeskRepository::new(&state.db)
                .ban_user(target_user, None, reason)
                .await?;
            users::reload(state, target_user).await?;
            DeskRepository::new(&state.db)
                .set_report_status(id, "banned")
                .await?;
        }
        _ => return Err(AppError::Validation("unknown report action".into())),
    }
    get_report(state, actor_id, id).await
}

pub async fn ban_user(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
    until: Option<DateTime<Utc>>,
    reason: &str,
) -> Result<User, AppError> {
    require_admin(state, actor_id).await?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation("ban reason is required".into()));
    }
    DeskRepository::new(&state.db)
        .ban_user(user_id, until, reason)
        .await?;
    users::reload(state, user_id).await
}

pub async fn unban_user(state: &AppState, actor_id: i64, user_id: i64) -> Result<User, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).unban_user(user_id).await?;
    users::reload(state, user_id).await
}

pub async fn self_unban(state: &AppState, user_id: i64) -> Result<User, AppError> {
    let user = users::get_profile(state, user_id).await?;
    if !user.banned {
        return Ok(user);
    }
    if user.banned_until.is_some_and(|until| until <= Utc::now()) {
        DeskRepository::new(&state.db).unban_user(user_id).await?;
        return users::reload(state, user_id).await;
    }
    Err(AppError::Banned(
        user.ban_reason
            .unwrap_or_else(|| "account is banned".into()),
    ))
}

pub async fn support_ban(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
    reason: &str,
) -> Result<User, AppError> {
    require_agent(state, actor_id).await?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation("reason is required".into()));
    }
    DeskRepository::new(&state.db)
        .support_ban(user_id, reason)
        .await?;
    users::reload(state, user_id).await
}

pub async fn support_unban(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
) -> Result<User, AppError> {
    require_agent(state, actor_id).await?;
    DeskRepository::new(&state.db)
        .support_unban(user_id)
        .await?;
    users::reload(state, user_id).await
}

pub async fn warn_user(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
    reason: &str,
) -> Result<Warning, AppError> {
    require_admin(state, actor_id).await?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::Validation("warning reason is required".into()));
    }
    users::get_profile(state, user_id).await?;
    let id = DeskRepository::new(&state.db)
        .add_warning(user_id, actor_id, reason)
        .await?;
    Ok(Warning {
        id,
        user_id,
        actor_id,
        reason: reason.to_owned(),
        created_at: Utc::now(),
    })
}

pub async fn list_warnings(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
) -> Result<Vec<Warning>, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).list_warnings(user_id).await
}

pub async fn set_limits(
    state: &AppState,
    actor_id: i64,
    user_id: i64,
    posting_allowed: bool,
    messaging_allowed: bool,
) -> Result<User, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db)
        .set_limits(user_id, posting_allowed, messaging_allowed)
        .await?;
    users::reload(state, user_id).await
}

pub async fn list_banned_links(
    state: &AppState,
    actor_id: i64,
) -> Result<Vec<BannedLink>, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).list_banned_links().await
}

pub async fn get_banned_link(state: &AppState, id: i64) -> Result<BannedLink, AppError> {
    DeskRepository::new(&state.db)
        .get_banned_link(id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn check_url(state: &AppState, url: &str) -> Result<Option<BannedLink>, AppError> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    DeskRepository::new(&state.db).find_banned_link(url).await
}

pub async fn add_banned_link(
    state: &AppState,
    actor_id: i64,
    url: &str,
    reason: &str,
) -> Result<BannedLink, AppError> {
    require_admin(state, actor_id).await?;
    let url = url.trim();
    let reason = reason.trim();
    if url.is_empty() || reason.is_empty() {
        return Err(AppError::Validation("url and reason are required".into()));
    }
    DeskRepository::new(&state.db)
        .add_banned_link(url, reason)
        .await
}

pub async fn delete_banned_link(state: &AppState, actor_id: i64, id: i64) -> Result<(), AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).delete_banned_link(id).await
}

pub async fn nospam(
    state: &AppState,
    actor_id: i64,
    query: &str,
    delete_hits: bool,
    ban_authors: bool,
) -> Result<NospamResult, AppError> {
    require_admin(state, actor_id).await?;
    let query = query.trim();
    if query.chars().count() < 3 {
        return Err(AppError::Validation("search query is too short".into()));
    }
    let repo = DeskRepository::new(&state.db);
    let hits = repo.search_wall(query).await?;
    let ids: Vec<i64> = hits.iter().map(|hit| hit.post_id).collect();
    let mut deleted = 0;
    let mut action_id = 0;
    if delete_hits && !ids.is_empty() {
        deleted = repo.delete_posts(&ids).await?;
        action_id = repo.save_nospam_action(actor_id, query, &ids).await?;
        if ban_authors {
            let mut seen = std::collections::HashSet::new();
            for hit in &hits {
                if seen.insert(hit.author_id) {
                    repo.ban_user(hit.author_id, None, &format!("noSpam: {query}"))
                        .await?;
                    users::reload(state, hit.author_id).await?;
                }
            }
        }
    }
    Ok(NospamResult {
        action_id,
        hits,
        deleted,
    })
}

pub async fn nospam_rollback(state: &AppState, actor_id: i64, id: i64) -> Result<(), AppError> {
    require_admin(state, actor_id).await?;
    let ids = DeskRepository::new(&state.db)
        .nospam_action_ids(id)
        .await?
        .ok_or(AppError::NotFound)?;
    DeskRepository::new(&state.db).restore_posts(&ids).await
}

pub async fn overview(state: &AppState, actor_id: i64) -> Result<AdminOverview, AppError> {
    require_admin(state, actor_id).await?;
    DeskRepository::new(&state.db).overview().await
}

pub async fn admin_users(
    state: &AppState,
    actor_id: i64,
    query: Option<&str>,
) -> Result<Vec<User>, AppError> {
    require_admin(state, actor_id).await?;
    let repo = DeskRepository::new(&state.db);
    let ids = if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        repo.search_user_ids(query).await?
    } else {
        repo.list_user_ids().await?
    };
    let map = load_users(state, &ids).await?;
    Ok(ids
        .into_iter()
        .filter_map(|id| map.get(&id).cloned())
        .collect())
}

pub async fn admin_clubs(state: &AppState, actor_id: i64) -> Result<Vec<Group>, AppError> {
    require_admin(state, actor_id).await?;
    crate::modules::groups::services::list(state).await
}

async fn hydrate_tickets(
    state: &AppState,
    rows: Vec<repository::TicketRow>,
    with_replies: bool,
) -> Result<Vec<Ticket>, AppError> {
    let mut author_ids: Vec<i64> = rows.iter().map(|row| row.author_id).collect();
    let mut reply_rows: HashMap<i64, Vec<repository::ReplyRow>> = HashMap::new();
    if with_replies {
        for row in &rows {
            let replies = DeskRepository::new(&state.db)
                .ticket_replies(row.id)
                .await?;
            author_ids.extend(replies.iter().map(|reply| reply.author_id));
            reply_rows.insert(row.id, replies);
        }
    }
    let map = load_users(state, &author_ids).await?;
    let mut tickets = Vec::new();
    for row in rows {
        let replies = reply_rows
            .remove(&row.id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|reply| {
                let author = map.get(&reply.author_id).cloned()?;
                Some(TicketReply {
                    id: reply.id,
                    ticket_id: reply.ticket_id,
                    author_id: reply.author_id,
                    author,
                    content: reply.content,
                    from_agent: reply.from_agent,
                    created_at: reply.created_at,
                })
            })
            .collect();
        let Some(author) = map.get(&row.author_id).cloned() else {
            continue;
        };
        tickets.push(Ticket {
            id: row.id,
            author_id: row.author_id,
            author,
            subject: row.subject,
            content: row.content,
            status: row.status,
            created_at: row.created_at,
            replies,
        });
    }
    Ok(tickets)
}

async fn hydrate_report(state: &AppState, row: repository::ReportRow) -> Result<Report, AppError> {
    let author = users::get_profile(state, row.author_id).await?;
    Ok(Report {
        id: row.id,
        author_id: row.author_id,
        author,
        target_type: row.target_type,
        target_id: row.target_id,
        reason: row.reason,
        status: row.status,
        created_at: row.created_at,
    })
}

async fn load_users(state: &AppState, ids: &[i64]) -> Result<HashMap<i64, User>, AppError> {
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if *id > 0 && seen.insert(*id) {
            unique.push(*id);
        }
    }
    let users = state.users().find_many(&unique).await?;
    Ok(users.into_iter().map(|user| (user.id, user)).collect())
}

async fn wall_author(state: &AppState, post_id: i64) -> Option<i64> {
    let row = sea_orm::ConnectionTrait::query_one_raw(
        &state.db,
        sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Postgres,
            "SELECT author_id FROM wall_posts WHERE id = $1",
            [post_id.into()],
        ),
    )
    .await
    .ok()??;
    row.try_get_by_index(0).ok()
}

fn normalize_serial(serial: &str) -> Result<String, AppError> {
    let compact: String = serial
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    if compact.len() != 24 {
        return Err(AppError::Validation(
            "voucher must be XXXXXX-XXXXXX-XXXXXX-XXXXXX".into(),
        ));
    }
    Ok(format!(
        "{}-{}-{}-{}",
        &compact[0..6],
        &compact[6..12],
        &compact[12..18],
        &compact[18..24]
    ))
}

fn random_serial() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    let mut token = String::with_capacity(27);
    for group in 0..4 {
        if group > 0 {
            token.push('-');
        }
        for _ in 0..6 {
            token.push(CHARS[rng.random_range(0..CHARS.len())] as char);
        }
    }
    token
}
