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
    #[serde(default)]
    pub privacy_profile: PrivacyLevel,
    #[serde(default)]
    pub privacy_friends: PrivacyLevel,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub coins: i64,
    #[serde(default)]
    pub rating: i32,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub banned: bool,
    #[serde(default)]
    pub ban_reason: Option<String>,
    #[serde(default)]
    pub banned_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub support_banned: bool,
    #[serde(default)]
    pub support_ban_reason: Option<String>,
    #[serde(default = "default_true")]
    pub posting_allowed: bool,
    #[serde(default = "default_true")]
    pub messaging_allowed: bool,
}

fn default_role() -> String {
    "user".into()
}

const fn default_true() -> bool {
    true
}

impl User {
    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    #[must_use]
    pub fn is_agent(&self) -> bool {
        matches!(self.role.as_str(), "admin" | "agent")
    }

    #[must_use]
    pub fn is_banned_now(&self) -> bool {
        if !self.banned {
            return false;
        }
        self.banned_until.is_none_or(|until| until > Utc::now())
    }
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
    pub privacy_photos: PrivacyLevel,
    pub privacy_audio: PrivacyLevel,
    pub privacy_profile: PrivacyLevel,
    pub privacy_friends: PrivacyLevel,
    /// `None` leaves the current status. `Some("")` clears it.
    pub status: Option<String>,
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
