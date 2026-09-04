use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub about: Option<String>,
    pub kind: String,
    pub owner_id: i64,
    pub created_at: DateTime<Utc>,
}
