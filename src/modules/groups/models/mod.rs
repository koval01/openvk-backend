use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub about: Option<String>,
    pub kind: String,
    pub owner_id: i64,
    pub created_at: DateTime<Utc>,
    pub avatar_url: Option<String>,
    pub members: i64,
    pub wall_open: bool,
}
