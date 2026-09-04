use chrono::{DateTime, Utc};

use crate::modules::users::User;

#[derive(Clone, Debug)]
pub struct Gift {
    pub id: i64,
    pub category_id: i64,
    pub name: String,
    pub description: String,
    pub price: i64,
    pub image_url: String,
}

#[derive(Clone, Debug)]
pub struct GiftCategory {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub gifts: Vec<Gift>,
}

#[derive(Clone, Debug)]
pub struct UserGift {
    pub id: i64,
    pub gift_id: i64,
    pub gift: Gift,
    pub sender_id: i64,
    pub sender: Option<User>,
    pub receiver_id: i64,
    pub caption: Option<String>,
    pub anonymous: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct TicketReply {
    pub id: i64,
    pub ticket_id: i64,
    pub author_id: i64,
    pub author: User,
    pub content: String,
    pub from_agent: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Ticket {
    pub id: i64,
    pub author_id: i64,
    pub author: User,
    pub subject: String,
    pub content: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub replies: Vec<TicketReply>,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub id: i64,
    pub author_id: i64,
    pub author: User,
    pub target_type: String,
    pub target_id: i64,
    pub reason: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Voucher {
    pub id: i64,
    pub serial: String,
    pub coins: i64,
    pub remaining: i32,
    pub total: i32,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct BannedLink {
    pub id: i64,
    pub url: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Warning {
    pub id: i64,
    pub user_id: i64,
    pub actor_id: i64,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NospamHit {
    pub post_id: i64,
    pub target_id: i64,
    pub local_id: i64,
    pub author_id: i64,
    pub content: String,
    pub permalink: String,
}

#[derive(Clone, Debug)]
pub struct NospamResult {
    pub action_id: i64,
    pub hits: Vec<NospamHit>,
    pub deleted: i32,
}

#[derive(Clone, Debug, Default)]
pub struct AdminOverview {
    pub users: i64,
    pub groups: i64,
    pub wall_posts: i64,
    pub tickets_open: i64,
    pub reports_open: i64,
    pub banned_users: i64,
}
