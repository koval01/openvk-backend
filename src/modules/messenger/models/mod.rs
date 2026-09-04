use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub peer_id: i64,
    pub author_id: i64,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SendMessage {
    pub peer_id: i64,
    pub text: String,
}
