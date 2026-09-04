use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::modules::users::User;

#[derive(Clone, Debug, Serialize)]
pub struct Notification {
    pub id: i64,
    pub kind: String,
    pub actor_id: Option<i64>,
    pub actor: Option<User>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub payload: Value,
    pub href: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
