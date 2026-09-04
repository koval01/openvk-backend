use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::wall_permalink;
use crate::modules::groups::models::Group;
use crate::modules::users::User;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lng: f64,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WallAttachment {
    pub kind: String,
    pub owner_id: i64,
    pub object_id: i64,
    pub url: String,
    pub title: String,
    pub src: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WallPost {
    /// Sequential id on this owner's wall, used in `wall{owner}_{id}`.
    pub id: i64,
    pub target_id: i64,
    pub author_id: i64,
    pub author: User,
    pub target: User,
    pub content: String,
    pub permalink: String,
    pub created_at: DateTime<Utc>,
    pub attachments: Vec<WallAttachment>,
    pub geo: Option<GeoPoint>,
    pub source: Option<String>,
    pub nsfw: bool,
    pub comment_count: i32,
    pub club: Option<Group>,
    pub like_count: i32,
    pub liked: bool,
}

impl WallPost {
    pub fn assemble(
        local_id: i64,
        target_id: i64,
        author_id: i64,
        author: User,
        target: User,
        club: Option<Group>,
        content: String,
        created_at: DateTime<Utc>,
        attachments: Vec<WallAttachment>,
        geo: Option<GeoPoint>,
        source: Option<String>,
        nsfw: bool,
        comment_count: i32,
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
            attachments,
            geo,
            source,
            nsfw,
            comment_count,
            club,
            like_count: 0,
            liked: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WriteWall {
    pub content: String,
    pub attachments: Vec<WallAttachment>,
    pub geo: Option<GeoPoint>,
    pub source: Option<String>,
    pub nsfw: bool,
}
