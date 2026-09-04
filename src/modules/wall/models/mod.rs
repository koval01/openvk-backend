use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::wall_permalink;
use crate::modules::users::User;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WallPost {
    /// Sequential id on this profile's wall, used in `wall{owner}_{id}`.
    pub id: i64,
    pub target_id: i64,
    pub author_id: i64,
    pub author: User,
    pub target: User,
    pub content: String,
    pub permalink: String,
    pub created_at: DateTime<Utc>,
}

impl WallPost {
    pub fn from_row(
        local_id: i64,
        target_id: i64,
        author_id: i64,
        author: User,
        target: User,
        content: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: local_id,
            target_id,
            author_id,
            author,
            target,
            content,
            permalink: wall_permalink(target_id, local_id),
            created_at,
        }
    }
}

#[derive(Deserialize)]
pub struct WriteWall {
    pub content: String,
}
