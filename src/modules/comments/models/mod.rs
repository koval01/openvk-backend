use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::modules::users::User;

#[derive(Clone, Debug, Serialize)]
pub struct Comment {
    pub id: i64,
    pub author_id: i64,
    pub author: User,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub like_count: i32,
    pub liked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentTarget {
    Wall,
    Photo,
    Video,
}

impl CommentTarget {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Photo => "photo",
            Self::Video => "video",
        }
    }
}
