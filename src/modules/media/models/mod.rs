use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::kinds::public_media_url;

#[derive(Clone, Debug, Serialize)]
pub struct AudioTrack {
    pub id: i64,
    pub media_id: i64,
    pub artist: String,
    pub title: String,
    pub duration_ms: i32,
    pub owner_user_id: i64,
    pub src: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Photo {
    pub id: i64,
    pub album_id: i64,
    pub owner_user_id: i64,
    pub mime: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub owner_user_id: i64,
    pub created_at: DateTime<Utc>,
    pub photo_count: i64,
    pub cover_url: Option<String>,
    pub photos: Vec<Photo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Video {
    pub id: i64,
    pub media_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub owner_user_id: i64,
    pub src: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateAlbum {
    pub title: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct OwnerQuery {
    pub owner_id: Option<i64>,
    pub photos: Option<bool>,
}

#[derive(Deserialize)]
pub struct UploadPhotoQuery {
    pub album_id: Option<i64>,
}

pub fn audio_from_row(
    id: i64,
    media_id: i64,
    artist: String,
    title: String,
    duration_ms: i32,
    owner_user_id: i64,
    storage_key: &str,
) -> AudioTrack {
    AudioTrack {
        id,
        media_id,
        artist,
        title,
        duration_ms,
        owner_user_id,
        src: public_media_url(storage_key),
    }
}

pub fn photo_from_media(album_id: i64, media: &crate::db::entities::media_object::Model) -> Photo {
    Photo {
        id: media.id,
        album_id,
        owner_user_id: media.owner_user_id,
        mime: media.mime.clone(),
        size_bytes: media.size_bytes,
        width: media.width,
        height: media.height,
        original_filename: media.original_filename.clone(),
        url: public_media_url(&media.storage_key),
    }
}

pub fn video_from_row(
    id: i64,
    media_id: Option<i64>,
    title: String,
    description: Option<String>,
    status: String,
    owner_user_id: i64,
    storage_key: Option<&str>,
) -> Video {
    Video {
        id,
        src: storage_key.map(public_media_url),
        media_id,
        title,
        description,
        status,
        owner_user_id,
    }
}
