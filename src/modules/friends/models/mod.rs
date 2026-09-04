use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FriendshipStatus {
    Incoming,
    Outgoing,
    Mutual,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Friendship {
    pub user_id: i64,
    pub peer_id: i64,
    pub status: FriendshipStatus,
    pub created_at: DateTime<Utc>,
}
