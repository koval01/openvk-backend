use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::media::kinds::infer_mime;
use crate::modules::media::models::{OwnerQuery, UploadPhotoQuery};
use crate::modules::media::services::{self, PreparedUpload};
use crate::pb;
use crate::state::AppState;
use crate::storage::validate_storage_key;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

pub async fn list_audio(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<OwnerQuery>,
) -> Result<Proto<pb::AudioList>, AppError> {
    Ok(Proto(codec::audio_list_to_pb(
        services::list_audio(&state, Some(query.owner_id.unwrap_or(auth.user_id))).await?,
    )))
}

pub async fn list_albums(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<OwnerQuery>,
) -> Result<Proto<pb::AlbumList>, AppError> {
    Ok(Proto(codec::albums_to_pb(
        services::list_albums(
            &state,
            Some(query.owner_id.unwrap_or(auth.user_id)),
            query.photos.unwrap_or(false),
        )
        .await?,
    )))
}

pub async fn get_album(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Album>, AppError> {
    Ok(Proto(codec::album_to_pb(
        &services::get_album(&state, id).await?,
    )))
}

pub async fn create_album(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::CreateAlbum>,
) -> Result<impl IntoResponse, AppError> {
    Ok((
        StatusCode::CREATED,
        Proto(codec::album_to_pb(
            &services::create_album(
                &state,
                auth.user_id,
                &body.title,
                body.description.as_deref(),
            )
            .await?,
        )),
    ))
}

pub async fn list_videos(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<OwnerQuery>,
) -> Result<Proto<pb::VideoList>, AppError> {
    Ok(Proto(codec::videos_to_pb(
        services::list_videos(&state, Some(query.owner_id.unwrap_or(auth.user_id))).await?,
    )))
}

pub async fn upload_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<UploadPhotoQuery>,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let upload = read_upload(multipart).await?;
    Ok((
        StatusCode::CREATED,
        Proto(codec::photo_to_pb(
            &services::upload_photo(&state, auth.user_id, query.album_id, upload).await?,
        )),
    ))
}

pub async fn upload_album_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(album_id): Path<i64>,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let upload = read_upload(multipart).await?;
    Ok((
        StatusCode::CREATED,
        Proto(codec::photo_to_pb(
            &services::upload_photo(&state, auth.user_id, Some(album_id), upload).await?,
        )),
    ))
}

pub async fn upload_audio(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let upload = read_upload(multipart).await?;
    Ok((
        StatusCode::CREATED,
        Proto(codec::audio_to_pb(
            &services::upload_audio(&state, auth.user_id, upload).await?,
        )),
    ))
}

pub async fn upload_video(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let upload = read_upload(multipart).await?;
    Ok((
        StatusCode::CREATED,
        Proto(codec::video_to_pb(
            &services::upload_video(&state, auth.user_id, upload).await?,
        )),
    ))
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<Proto<pb::User>, AppError> {
    let upload = read_upload(multipart).await?;
    Ok(Proto(codec::user_to_pb(
        &services::upload_avatar(&state, auth.user_id, upload).await?,
    )))
}

pub async fn delete_album(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::delete_album(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_album_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((album_id, media_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    services::delete_photo(&state, auth.user_id, album_id, media_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_audio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::delete_audio(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_video(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    services::delete_video(&state, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn serve_object(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Response, AppError> {
    if validate_storage_key(&key).is_err() {
        return Err(AppError::NotFound);
    }
    let object = state.storage.get(&key).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&object.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    Ok((headers, object.bytes).into_response())
}

async fn read_upload(mut multipart: Multipart) -> Result<PreparedUpload, AppError> {
    let mut file_name = String::new();
    let mut declared_mime = None;
    let mut bytes = None;
    let mut title = None;
    let mut artist = None;
    let mut description = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Validation(format!("invalid multipart body: {err}")))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "file" | "photo" | "audio" | "video" | "avatar" => {
                file_name = field
                    .file_name()
                    .map(ToOwned::to_owned)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "upload.bin".into());
                declared_mime = field.content_type().map(ToOwned::to_owned);
                bytes =
                    Some(field.bytes().await.map_err(|err| {
                        AppError::Validation(format!("could not read file: {err}"))
                    })?);
            }
            "title" => title = Some(field_text(field).await?),
            "artist" => artist = Some(field_text(field).await?),
            "description" => description = Some(field_text(field).await?),
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let bytes = bytes.ok_or_else(|| AppError::Validation("file is required".into()))?;
    if bytes.is_empty() {
        return Err(AppError::Validation("file is empty".into()));
    }
    Ok(PreparedUpload {
        mime: infer_mime(&file_name, declared_mime.as_deref()),
        file_name,
        bytes,
        title,
        artist,
        description,
    })
}

async fn field_text(field: axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    field
        .text()
        .await
        .map_err(|err| AppError::Validation(format!("invalid form field: {err}")))
}
