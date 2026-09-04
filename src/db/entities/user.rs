use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub login: String,
    pub password_hash: String,
    pub password_algo: String,
    pub first_name: String,
    pub last_name: String,
    pub screen_name: Option<String>,
    pub status: Option<String>,
    pub city: Option<String>,
    pub email: Option<String>,
    pub email_idx: Option<String>,
    pub phone: Option<String>,
    pub phone_idx: Option<String>,
    pub wrap_key: Option<String>,
    /// Storage key of the avatar object. The public URL is built from config.
    pub avatar_key: Option<String>,
    pub verified: bool,
    pub privacy_wall: String,
    pub privacy_messages: String,
    pub privacy_profile: String,
    pub privacy_photos: String,
    pub privacy_audio: String,
    pub email_verified_at: Option<DateTimeUtc>,
    pub last_seen_at: Option<DateTimeUtc>,
    pub wall_seq: i64,
    pub coins: i64,
    pub rating: i32,
    pub role: String,
    pub banned: bool,
    pub banned_until: Option<DateTimeUtc>,
    pub ban_reason: Option<String>,
    pub support_banned: bool,
    pub support_ban_reason: Option<String>,
    pub posting_allowed: bool,
    pub messaging_allowed: bool,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
