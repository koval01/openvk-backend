use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};

use crate::db::entities::{album, album_photo, audio, media_object, video};
use crate::error::AppError;
use crate::ids::insert_with_random_id;
use crate::modules::media::kinds::{MediaKind, public_media_url};
use crate::modules::media::models::{
    Album, AudioTrack, Photo, Video, audio_from_row, photo_from_media, video_from_row,
};

pub struct MediaRepository<'a> {
    db: &'a DatabaseConnection,
}

pub struct NewMedia {
    pub owner_user_id: i64,
    pub kind: MediaKind,
    pub storage_key: String,
    pub mime: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: Option<String>,
}

impl<'a> MediaRepository<'a> {
    pub const fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list_audio(
        &self,
        owner_user_id: Option<i64>,
    ) -> Result<Vec<AudioTrack>, AppError> {
        let mut query = audio::Entity::find().order_by_desc(audio::Column::CreatedAt);
        if let Some(owner_user_id) = owner_user_id {
            query = query.filter(audio::Column::OwnerUserId.eq(owner_user_id));
        }
        let rows = query.limit(50).all(self.db).await?;
        let keys = self
            .storage_keys_by_ids(&rows.iter().map(|row| row.media_id).collect::<Vec<_>>())
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                keys.get(&row.media_id).map(|key| {
                    audio_from_row(
                        row.id,
                        row.media_id,
                        row.artist,
                        row.title,
                        row.duration_ms,
                        row.owner_user_id,
                        key,
                    )
                })
            })
            .collect())
    }

    pub async fn list_albums(
        &self,
        owner_user_id: Option<i64>,
        include_photos: bool,
    ) -> Result<Vec<Album>, AppError> {
        let mut query = album::Entity::find().order_by_desc(album::Column::CreatedAt);
        if let Some(owner_user_id) = owner_user_id {
            query = query.filter(album::Column::OwnerUserId.eq(owner_user_id));
        }
        let rows = query.limit(50).all(self.db).await?;
        let mut albums = Vec::with_capacity(rows.len());
        for row in rows {
            albums.push(self.hydrate_album(row, include_photos).await?);
        }
        Ok(albums)
    }

    pub async fn get_album(&self, album_id: i64) -> Result<Album, AppError> {
        let row = album::Entity::find_by_id(album_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        self.hydrate_album(row, true).await
    }

    pub async fn list_videos(&self, owner_user_id: Option<i64>) -> Result<Vec<Video>, AppError> {
        let mut query = video::Entity::find().order_by_desc(video::Column::CreatedAt);
        if let Some(owner_user_id) = owner_user_id {
            query = query.filter(video::Column::OwnerUserId.eq(owner_user_id));
        }
        let rows = query.limit(50).all(self.db).await?;
        let keys = self
            .storage_keys_by_ids(
                &rows
                    .iter()
                    .filter_map(|row| row.media_id)
                    .collect::<Vec<_>>(),
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                video_from_row(
                    row.id,
                    row.media_id,
                    row.title,
                    row.description,
                    row.status,
                    row.owner_user_id,
                    row.media_id
                        .and_then(|media_id| keys.get(&media_id).map(String::as_str)),
                )
            })
            .collect())
    }

    pub async fn create_album(
        &self,
        owner_user_id: i64,
        title: &str,
        description: Option<&str>,
    ) -> Result<Album, AppError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::Validation("album title is required".into()));
        }
        let description = description
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned);
        let inserted = insert_with_random_id(|id| {
            album::ActiveModel {
                id: Set(id),
                owner_user_id: Set(owner_user_id),
                title: Set(title.to_owned()),
                description: Set(description.clone()),
                privacy: Set("everyone".into()),
                created_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(self.db)
        })
        .await?;
        self.hydrate_album(inserted, true).await
    }

    pub async fn default_album(&self, owner_user_id: i64) -> Result<album::Model, AppError> {
        if let Some(existing) = album::Entity::find()
            .filter(album::Column::OwnerUserId.eq(owner_user_id))
            .order_by_asc(album::Column::CreatedAt)
            .one(self.db)
            .await?
        {
            return Ok(existing);
        }
        insert_with_random_id(|id| {
            album::ActiveModel {
                id: Set(id),
                owner_user_id: Set(owner_user_id),
                title: Set("Photos".into()),
                privacy: Set("everyone".into()),
                created_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(self.db)
        })
        .await
    }

    pub async fn require_owned_album(
        &self,
        album_id: i64,
        owner_user_id: i64,
    ) -> Result<album::Model, AppError> {
        let album = album::Entity::find_by_id(album_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        if album.owner_user_id != owner_user_id {
            return Err(AppError::Forbidden);
        }
        Ok(album)
    }

    pub async fn insert_media(&self, new_media: NewMedia) -> Result<media_object::Model, AppError> {
        insert_with_random_id(|id| {
            media_object::ActiveModel {
                id: Set(id),
                owner_user_id: Set(new_media.owner_user_id),
                kind: Set(new_media.kind.as_str().to_owned()),
                storage_key: Set(new_media.storage_key.clone()),
                mime: Set(new_media.mime.clone()),
                size_bytes: Set(new_media.size_bytes),
                width: Set(new_media.width),
                height: Set(new_media.height),
                original_filename: Set(new_media.original_filename.clone()),
                created_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(self.db)
        })
        .await
    }

    pub async fn attach_photo(
        &self,
        album: &album::Model,
        media: &media_object::Model,
    ) -> Result<Photo, AppError> {
        let next = album_photo::Entity::find()
            .filter(album_photo::Column::AlbumId.eq(album.id))
            .count(self.db)
            .await?;
        let position = i32::try_from(next).unwrap_or(i32::MAX);
        album_photo::ActiveModel {
            album_id: Set(album.id),
            media_id: Set(media.id),
            position: Set(position),
        }
        .insert(self.db)
        .await?;
        if album.cover_media_id.is_none() {
            let mut active: album::ActiveModel = album.clone().into();
            active.cover_media_id = Set(Some(media.id));
            active.update(self.db).await?;
        }
        Ok(photo_from_media(album.id, media))
    }

    pub async fn insert_audio(
        &self,
        owner_user_id: i64,
        media_id: i64,
        artist: &str,
        title: &str,
        duration_ms: i32,
        storage_key: &str,
    ) -> Result<AudioTrack, AppError> {
        let artist = empty_to("Unknown artist", artist);
        let title = empty_to("Untitled", title);
        let row = insert_with_random_id(|id| {
            audio::ActiveModel {
                id: Set(id),
                media_id: Set(media_id),
                owner_user_id: Set(owner_user_id),
                artist: Set(artist.clone()),
                title: Set(title.clone()),
                duration_ms: Set(duration_ms.max(0)),
                listens: Set(0),
                created_at: Set(Utc::now()),
                ..Default::default()
            }
            .insert(self.db)
        })
        .await?;
        Ok(audio_from_row(
            row.id,
            row.media_id,
            row.artist,
            row.title,
            row.duration_ms,
            row.owner_user_id,
            storage_key,
        ))
    }

    pub async fn insert_video(
        &self,
        owner_user_id: i64,
        media_id: i64,
        title: &str,
        description: Option<&str>,
        storage_key: &str,
    ) -> Result<Video, AppError> {
        let title = empty_to("Untitled video", title);
        let description = description
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned);
        let row = insert_with_random_id(|id| {
            video::ActiveModel {
                id: Set(id),
                media_id: Set(Some(media_id)),
                owner_user_id: Set(owner_user_id),
                title: Set(title.clone()),
                description: Set(description.clone()),
                status: Set("ready".into()),
                created_at: Set(Utc::now()),
            }
            .insert(self.db)
        })
        .await?;
        Ok(video_from_row(
            row.id,
            row.media_id,
            row.title,
            row.description,
            row.status,
            row.owner_user_id,
            Some(storage_key),
        ))
    }

    pub async fn get_media(&self, media_id: i64) -> Result<media_object::Model, AppError> {
        media_object::Entity::find_by_id(media_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)
    }

    pub async fn storage_keys_for_user(&self, owner_user_id: i64) -> Result<Vec<String>, AppError> {
        let rows = media_object::Entity::find()
            .filter(media_object::Column::OwnerUserId.eq(owner_user_id))
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|row| row.storage_key).collect())
    }

    pub async fn storage_keys_for_album(&self, album_id: i64) -> Result<Vec<String>, AppError> {
        let links = album_photo::Entity::find()
            .filter(album_photo::Column::AlbumId.eq(album_id))
            .all(self.db)
            .await?;
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = links.into_iter().map(|row| row.media_id).collect();
        let rows = media_object::Entity::find()
            .filter(media_object::Column::Id.is_in(ids))
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|row| row.storage_key).collect())
    }

    pub async fn find_audio(
        &self,
        audio_id: i64,
    ) -> Result<(audio::Model, media_object::Model), AppError> {
        let audio = audio::Entity::find_by_id(audio_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let media = self.get_media(audio.media_id).await?;
        Ok((audio, media))
    }

    pub async fn find_video(
        &self,
        video_id: i64,
    ) -> Result<(video::Model, Option<media_object::Model>), AppError> {
        let video = video::Entity::find_by_id(video_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let media = if let Some(media_id) = video.media_id {
            Some(self.get_media(media_id).await?)
        } else {
            None
        };
        Ok((video, media))
    }

    pub async fn find_avatar(
        &self,
        owner_user_id: i64,
    ) -> Result<Option<media_object::Model>, AppError> {
        Ok(media_object::Entity::find()
            .filter(media_object::Column::OwnerUserId.eq(owner_user_id))
            .filter(media_object::Column::Kind.eq(MediaKind::Avatar.as_str()))
            .order_by_desc(media_object::Column::CreatedAt)
            .one(self.db)
            .await?)
    }

    pub async fn delete_media_row(&self, media_id: i64) -> Result<(), AppError> {
        let result = media_object::Entity::delete_by_id(media_id)
            .exec(self.db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn delete_album_row(&self, album_id: i64) -> Result<(), AppError> {
        let links = album_photo::Entity::find()
            .filter(album_photo::Column::AlbumId.eq(album_id))
            .all(self.db)
            .await?;
        let txn = self.db.begin().await?;
        album::Entity::delete_by_id(album_id).exec(&txn).await?;
        if !links.is_empty() {
            let ids: Vec<i64> = links.into_iter().map(|row| row.media_id).collect();
            media_object::Entity::delete_many()
                .filter(media_object::Column::Id.is_in(ids))
                .exec(&txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn delete_audio_row(&self, audio_id: i64) -> Result<i64, AppError> {
        let audio = audio::Entity::find_by_id(audio_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let media_id = audio.media_id;
        audio::Entity::delete_by_id(audio_id).exec(self.db).await?;
        media_object::Entity::delete_by_id(media_id)
            .exec(self.db)
            .await?;
        Ok(media_id)
    }

    pub async fn delete_video_row(&self, video_id: i64) -> Result<Option<i64>, AppError> {
        let video = video::Entity::find_by_id(video_id)
            .one(self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let media_id = video.media_id;
        video::Entity::delete_by_id(video_id).exec(self.db).await?;
        if let Some(media_id) = media_id {
            media_object::Entity::delete_by_id(media_id)
                .exec(self.db)
                .await?;
        }
        Ok(media_id)
    }

    async fn hydrate_album(
        &self,
        row: album::Model,
        include_photos: bool,
    ) -> Result<Album, AppError> {
        let photo_count = album_photo::Entity::find()
            .filter(album_photo::Column::AlbumId.eq(row.id))
            .count(self.db)
            .await?;
        let photos = if include_photos {
            self.album_photos(row.id).await?
        } else {
            Vec::new()
        };
        let cover_url = match row.cover_media_id {
            Some(media_id) => match self.get_media(media_id).await {
                Ok(media) => Some(public_media_url(&media.storage_key)),
                Err(_) => photos.first().map(|photo| photo.url.clone()),
            },
            None => photos.first().map(|photo| photo.url.clone()),
        };
        Ok(Album {
            id: row.id,
            title: row.title,
            description: row.description,
            owner_user_id: row.owner_user_id,
            created_at: row.created_at,
            photo_count: i64::try_from(photo_count).unwrap_or(i64::MAX),
            cover_url,
            photos,
        })
    }

    async fn album_photos(&self, album_id: i64) -> Result<Vec<Photo>, AppError> {
        let links = album_photo::Entity::find()
            .filter(album_photo::Column::AlbumId.eq(album_id))
            .order_by_asc(album_photo::Column::Position)
            .all(self.db)
            .await?;
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = links.iter().map(|row| row.media_id).collect();
        let media_rows = media_object::Entity::find()
            .filter(media_object::Column::Id.is_in(ids.clone()))
            .all(self.db)
            .await?;
        let mut photos = Vec::with_capacity(links.len());
        for link in links {
            if let Some(media) = media_rows.iter().find(|row| row.id == link.media_id) {
                photos.push(photo_from_media(album_id, media));
            }
        }
        Ok(photos)
    }

    async fn storage_keys_by_ids(&self, ids: &[i64]) -> Result<HashMap<i64, String>, AppError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = media_object::Entity::find()
            .filter(media_object::Column::Id.is_in(ids.to_vec()))
            .all(self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.id, row.storage_key))
            .collect())
    }
}

fn empty_to(fallback: &str, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}
