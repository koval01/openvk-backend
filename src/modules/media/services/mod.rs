use bytes::Bytes;

use crate::error::AppError;
use crate::modules::media::inspect;
use crate::modules::media::kinds::{MediaKind, storage_key};
use crate::modules::media::models::{Album, AudioTrack, Photo, Video};
use crate::modules::media::repository::{MediaRepository, NewMedia};
use crate::modules::users;
use crate::modules::users::models::User;
use crate::state::AppState;

pub struct PreparedUpload {
    pub file_name: String,
    pub mime: String,
    pub bytes: Bytes,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
}

pub async fn list_audio(
    state: &AppState,
    owner_user_id: Option<i64>,
) -> Result<Vec<AudioTrack>, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .list_audio(owner_user_id)
        .await
}

pub async fn list_albums(
    state: &AppState,
    owner_user_id: Option<i64>,
    include_photos: bool,
) -> Result<Vec<Album>, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .list_albums(owner_user_id, include_photos)
        .await
}

pub async fn get_album(state: &AppState, album_id: i64) -> Result<Album, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_album(album_id)
        .await
}

pub async fn list_videos(
    state: &AppState,
    owner_user_id: Option<i64>,
) -> Result<Vec<Video>, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .list_videos(owner_user_id)
        .await
}

pub async fn get_photo(
    state: &AppState,
    owner_user_id: i64,
    media_id: i64,
) -> Result<Photo, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_photo(owner_user_id, media_id)
        .await
}

pub async fn get_photo_by_id(state: &AppState, media_id: i64) -> Result<Photo, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_photo_by_id(media_id)
        .await
}

pub async fn get_video(
    state: &AppState,
    owner_user_id: i64,
    video_id: i64,
) -> Result<Video, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_video(owner_user_id, video_id)
        .await
}

pub async fn get_video_by_id(state: &AppState, video_id: i64) -> Result<Video, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_video_by_id(video_id)
        .await
}

pub async fn get_audio_by_id(state: &AppState, audio_id: i64) -> Result<AudioTrack, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .get_audio_by_id(audio_id)
        .await
}

pub async fn create_album(
    state: &AppState,
    owner_user_id: i64,
    title: &str,
    description: Option<&str>,
) -> Result<Album, AppError> {
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .create_album(owner_user_id, title, description)
        .await
}

pub async fn upload_photo(
    state: &AppState,
    owner_user_id: i64,
    album_id: Option<i64>,
    upload: PreparedUpload,
) -> Result<Photo, AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    let existing_album = if let Some(album_id) = album_id {
        Some(repo.require_owned_album(album_id, owner_user_id).await?)
    } else {
        None
    };
    let media = persist_object(state, owner_user_id, MediaKind::Photo, &upload).await?;
    let album = match existing_album {
        Some(album) => album,
        None => repo.default_album(owner_user_id).await?,
    };
    repo.attach_photo(&album, &media).await
}

pub async fn upload_audio(
    state: &AppState,
    owner_user_id: i64,
    upload: PreparedUpload,
) -> Result<AudioTrack, AppError> {
    let media = persist_object(state, owner_user_id, MediaKind::Audio, &upload).await?;
    let artist = upload.artist.as_deref().unwrap_or("");
    let title = upload
        .title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&upload.file_name);
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .insert_audio(
            owner_user_id,
            media.id,
            artist,
            title,
            0,
            &media.storage_key,
        )
        .await
}

pub async fn upload_video(
    state: &AppState,
    owner_user_id: i64,
    upload: PreparedUpload,
) -> Result<Video, AppError> {
    let media = persist_object(state, owner_user_id, MediaKind::Video, &upload).await?;
    let title = upload
        .title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&upload.file_name);
    MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .insert_video(
            owner_user_id,
            media.id,
            title,
            upload.description.as_deref(),
            &media.storage_key,
        )
        .await
}

pub async fn upload_avatar(
    state: &AppState,
    owner_user_id: i64,
    upload: PreparedUpload,
) -> Result<User, AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    let previous = repo.find_avatar(owner_user_id).await?;
    let media = persist_object(state, owner_user_id, MediaKind::Avatar, &upload).await?;
    let user = state
        .users()
        .set_avatar_key(owner_user_id, Some(media.storage_key.clone()))
        .await?;
    users::services::replace_cached_user(state, user.clone()).await;
    let album = repo.default_album(owner_user_id).await?;
    repo.attach_photo(&album, &media).await?;
    if let Some(previous) = previous {
        state.storage.delete(&previous.storage_key).await?;
        repo.delete_media_row(previous.id).await?;
    }
    Ok(user)
}

pub async fn delete_album(
    state: &AppState,
    owner_user_id: i64,
    album_id: i64,
) -> Result<(), AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    repo.require_owned_album(album_id, owner_user_id).await?;
    let keys = repo.storage_keys_for_album(album_id).await?;
    state.storage.delete_keys(&keys).await?;
    repo.delete_album_row(album_id).await
}

pub async fn delete_photo(
    state: &AppState,
    owner_user_id: i64,
    album_id: i64,
    media_id: i64,
) -> Result<(), AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    repo.require_owned_album(album_id, owner_user_id).await?;
    let media = repo.get_media(media_id).await?;
    if media.owner_user_id != owner_user_id {
        return Err(AppError::Forbidden);
    }
    state.storage.delete(&media.storage_key).await?;
    repo.delete_media_row(media_id).await
}

pub async fn delete_audio(
    state: &AppState,
    owner_user_id: i64,
    audio_id: i64,
) -> Result<(), AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    let (audio, media) = repo.find_audio(audio_id).await?;
    if audio.owner_user_id != owner_user_id {
        return Err(AppError::Forbidden);
    }
    state.storage.delete(&media.storage_key).await?;
    repo.delete_audio_row(audio_id).await?;
    Ok(())
}

pub async fn delete_video(
    state: &AppState,
    owner_user_id: i64,
    video_id: i64,
) -> Result<(), AppError> {
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    let (video, media) = repo.find_video(video_id).await?;
    if video.owner_user_id != owner_user_id {
        return Err(AppError::Forbidden);
    }
    if let Some(media) = media {
        state.storage.delete(&media.storage_key).await?;
    }
    repo.delete_video_row(video_id).await?;
    Ok(())
}

pub async fn delete_all_user_objects(state: &AppState, owner_user_id: i64) -> Result<(), AppError> {
    let keys = MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .storage_keys_for_user(owner_user_id)
        .await?;
    state.storage.delete_keys(&keys).await
}

pub(crate) async fn persist_object(
    state: &AppState,
    owner_user_id: i64,
    kind: MediaKind,
    upload: &PreparedUpload,
) -> Result<crate::db::entities::media_object::Model, AppError> {
    let size = u64::try_from(upload.bytes.len()).unwrap_or(u64::MAX);
    kind.validate_size(size)?;
    if size > state.config.max_upload_bytes {
        return Err(AppError::Validation(
            "file exceeds server upload limit".into(),
        ));
    }
    let file_name = upload.file_name.clone();
    let declared_mime = upload.mime.clone();
    let raw = upload.bytes.clone();
    let clean = tokio::task::spawn_blocking(move || {
        inspect::sanitize(kind, &file_name, &declared_mime, &raw)
    })
    .await
    .map_err(|error| AppError::internal(format!("media inspect cancelled: {error}")))??;
    let clean_size = u64::try_from(clean.bytes.len()).unwrap_or(u64::MAX);
    if clean_size > state.config.max_upload_bytes {
        return Err(AppError::Validation(
            "file exceeds server upload limit".into(),
        ));
    }
    let key = storage_key(owner_user_id, kind, clean.extension);
    state
        .storage
        .put(&key, clean.bytes.clone(), &clean.mime)
        .await?;
    let size_bytes = i64::try_from(clean.bytes.len()).unwrap_or(i64::MAX);
    match MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .insert_media(NewMedia {
            owner_user_id,
            kind,
            storage_key: key.clone(),
            mime: clean.mime,
            size_bytes,
            width: clean.width,
            height: clean.height,
            original_filename: Some(upload.file_name.clone()),
        })
        .await
    {
        Ok(media) => Ok(media),
        Err(error) => {
            let _ = state.storage.delete(&key).await;
            Err(error)
        }
    }
}
