use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    #[default]
    Everyone,
    Friends,
    Nobody,
}

impl PrivacyLevel {
    #[must_use]
    pub fn from_db(value: &str) -> Self {
        match value {
            "friends" => Self::Friends,
            "nobody" => Self::Nobody,
            _ => Self::Everyone,
        }
    }

    #[must_use]
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Friends => "friends",
            Self::Nobody => "nobody",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub screen_name: Option<String>,
    pub status: Option<String>,
    pub city: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub privacy_wall: PrivacyLevel,
    pub privacy_messages: PrivacyLevel,
    pub privacy_photos: PrivacyLevel,
    pub privacy_audio: PrivacyLevel,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateAccount {
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub city: Option<String>,
    pub privacy_wall: PrivacyLevel,
    pub privacy_messages: PrivacyLevel,
}

#[derive(Deserialize)]
pub struct ChangePassword {
    pub challenge_id: String,
    pub password_sealed: String,
}

#[derive(Deserialize)]
pub struct DeleteAccount {
    pub challenge_id: String,
    pub password_sealed: String,
}
