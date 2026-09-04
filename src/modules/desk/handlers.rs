use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::desk::{codec as desk_codec, services};
use crate::pb;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub all: Option<bool>,
    pub q: Option<String>,
    pub url: Option<String>,
}

pub async fn gift_catalog(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Proto<pb::GiftCatalog>, AppError> {
    Ok(Proto(desk_codec::catalog_to_pb(
        services::catalog(&state).await?,
    )))
}

pub async fn user_gifts(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::UserGiftList>, AppError> {
    Ok(Proto(desk_codec::user_gifts_to_pb(
        services::user_gifts(&state, id).await?,
    )))
}

pub async fn send_gift(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::SendGift>,
) -> Result<impl IntoResponse, AppError> {
    let gift = services::send_gift(&state, auth.user_id, body).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::user_gift_to_pb(&gift)),
    ))
}

pub async fn transfer_coins(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::TransferCoins>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::transfer_coins(&state, auth.user_id, body).await?,
    )))
}

pub async fn redeem_voucher(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::RedeemVoucher>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::redeem_voucher(&state, auth.user_id, &body.serial).await?,
    )))
}

pub async fn list_vouchers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::VoucherList>, AppError> {
    Ok(Proto(desk_codec::vouchers_to_pb(
        services::list_vouchers(&state, auth.user_id).await?,
    )))
}

pub async fn get_voucher(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Voucher>, AppError> {
    Ok(Proto(desk_codec::voucher_to_pb(
        &services::get_voucher(&state, auth.user_id, id).await?,
    )))
}

pub async fn create_voucher(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::CreateVoucher>,
) -> Result<impl IntoResponse, AppError> {
    let voucher = services::create_voucher(&state, auth.user_id, body.coins, body.uses).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::voucher_to_pb(&voucher)),
    ))
}

pub async fn list_tickets(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Proto<pb::TicketList>, AppError> {
    Ok(Proto(desk_codec::tickets_to_pb(
        services::list_tickets(&state, auth.user_id, query.all.unwrap_or(false)).await?,
    )))
}

pub async fn create_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::WriteTicket>,
) -> Result<impl IntoResponse, AppError> {
    let ticket = services::create_ticket(&state, auth.user_id, body).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::ticket_to_pb(&ticket)),
    ))
}

pub async fn get_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Ticket>, AppError> {
    Ok(Proto(desk_codec::ticket_to_pb(
        &services::get_ticket(&state, auth.user_id, id).await?,
    )))
}

pub async fn reply_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::WriteTicketReply>,
) -> Result<Proto<pb::Ticket>, AppError> {
    Ok(Proto(desk_codec::ticket_to_pb(
        &services::reply_ticket(&state, auth.user_id, id, &body.content).await?,
    )))
}

pub async fn close_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Ticket>, AppError> {
    Ok(Proto(desk_codec::ticket_to_pb(
        &services::close_ticket(&state, auth.user_id, id).await?,
    )))
}

pub async fn delete_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::delete_ticket(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::WriteReport>,
) -> Result<impl IntoResponse, AppError> {
    let report = services::create_report(&state, auth.user_id, body).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::report_to_pb(&report)),
    ))
}

pub async fn list_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::ReportList>, AppError> {
    Ok(Proto(desk_codec::reports_to_pb(
        services::list_reports(&state, auth.user_id).await?,
    )))
}

pub async fn get_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Report>, AppError> {
    Ok(Proto(desk_codec::report_to_pb(
        &services::get_report(&state, auth.user_id, id).await?,
    )))
}

pub async fn report_action(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::ReportAction>,
) -> Result<Proto<pb::Report>, AppError> {
    Ok(Proto(desk_codec::report_to_pb(
        &services::report_action(
            &state,
            auth.user_id,
            id,
            &body.action,
            body.reason.as_deref(),
        )
        .await?,
    )))
}

pub async fn ban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::BanUser>,
) -> Result<Proto<pb::User>, AppError> {
    let until = parse_until(body.until)?;
    Ok(Proto(codec::user_to_pb(
        &services::ban_user(&state, auth.user_id, id, until, &body.reason).await?,
    )))
}

pub async fn unban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::unban_user(&state, auth.user_id, id).await?,
    )))
}

pub async fn self_unban(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::self_unban(&state, auth.user_id).await?,
    )))
}

pub async fn support_ban(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::BanUser>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::support_ban(&state, auth.user_id, id, &body.reason).await?,
    )))
}

pub async fn support_unban(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::support_unban(&state, auth.user_id, id).await?,
    )))
}

pub async fn warn_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::WarnUser>,
) -> Result<impl IntoResponse, AppError> {
    let warning = services::warn_user(&state, auth.user_id, id, &body.reason).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::warning_to_pb(&warning)),
    ))
}

pub async fn list_warnings(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::WarningList>, AppError> {
    Ok(Proto(desk_codec::warnings_to_pb(
        services::list_warnings(&state, auth.user_id, id).await?,
    )))
}

pub async fn set_limits(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::SetLimits>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::set_limits(
            &state,
            auth.user_id,
            id,
            body.posting_allowed,
            body.messaging_allowed,
        )
        .await?,
    )))
}

pub async fn list_banned_links(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::BannedLinkList>, AppError> {
    Ok(Proto(desk_codec::banned_links_to_pb(
        services::list_banned_links(&state, auth.user_id).await?,
    )))
}

pub async fn get_banned_link(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Proto<pb::BannedLink>, AppError> {
    Ok(Proto(desk_codec::banned_link_to_pb(
        &services::get_banned_link(&state, id).await?,
    )))
}

pub async fn check_url(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Proto<pb::BannedLinkList>, AppError> {
    let url = query.url.unwrap_or_default();
    let links = match services::check_url(&state, &url).await? {
        Some(link) => vec![link],
        None => Vec::new(),
    };
    Ok(Proto(desk_codec::banned_links_to_pb(links)))
}

pub async fn add_banned_link(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::WriteBannedLink>,
) -> Result<impl IntoResponse, AppError> {
    let link = services::add_banned_link(&state, auth.user_id, &body.url, &body.reason).await?;
    Ok((
        StatusCode::CREATED,
        Proto(desk_codec::banned_link_to_pb(&link)),
    ))
}

pub async fn delete_banned_link(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::delete_banned_link(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn nospam(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::NospamQuery>,
) -> Result<Proto<pb::NospamResult>, AppError> {
    Ok(Proto(desk_codec::nospam_to_pb(
        &services::nospam(
            &state,
            auth.user_id,
            &body.query,
            body.delete_hits,
            body.ban_authors,
        )
        .await?,
    )))
}

pub async fn nospam_rollback(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::nospam_rollback(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn overview(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::AdminOverview>, AppError> {
    Ok(Proto(desk_codec::overview_to_pb(
        &services::overview(&state, auth.user_id).await?,
    )))
}

pub async fn admin_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Proto<pb::AdminUserList>, AppError> {
    Ok(Proto(pb::AdminUserList {
        users: services::admin_users(&state, auth.user_id, query.q.as_deref())
            .await?
            .iter()
            .map(codec::user_to_pb)
            .collect(),
    }))
}

pub async fn admin_clubs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::AdminClubList>, AppError> {
    Ok(Proto(pb::AdminClubList {
        groups: services::admin_clubs(&state, auth.user_id)
            .await?
            .iter()
            .map(codec::group_to_pb)
            .collect(),
    }))
}

fn parse_until(value: Option<String>) -> Result<Option<DateTime<Utc>>, AppError> {
    let Some(value) = value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(&value)
        .map(|parsed| Some(parsed.with_timezone(&Utc)))
        .map_err(|_| AppError::Validation("invalid ban expiry".into()))
}
