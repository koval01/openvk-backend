use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::comments::models::CommentTarget;
use crate::modules::comments::services;
use crate::modules::media;
use crate::modules::wall;
use crate::pb;
use crate::state::AppState;

pub async fn list_wall_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::CommentList>, AppError> {
    let _post = wall::services::get_wall_post(&state, owner_id, local_id).await?;
    let mut comments = services::list(&state, CommentTarget::Wall, owner_id, local_id).await?;
    crate::modules::likes::services::attach_comments(&state, &mut comments, auth.user_id).await?;
    Ok(Proto(codec::comments_to_pb(comments)))
}

pub async fn write_wall_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, local_id)): Path<(i64, i64)>,
    Proto(body): Proto<pb::WriteComment>,
) -> Result<impl IntoResponse, AppError> {
    let post = wall::services::get_wall_post(&state, owner_id, local_id).await?;
    let comment = services::write(
        &state,
        auth.user_id,
        CommentTarget::Wall,
        owner_id,
        local_id,
        body.content,
        Some(if post.target_id > 0 {
            post.target_id
        } else {
            post.author_id
        }),
        &format!("/{}", post.permalink),
    )
    .await?;
    Ok((StatusCode::CREATED, Proto(codec::comment_to_pb(&comment))))
}

pub async fn list_wall_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((group_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::CommentList>, AppError> {
    list_wall_user(State(state), auth, Path((-group_id, local_id))).await
}

pub async fn write_wall_group(
    state: State<AppState>,
    auth: AuthUser,
    Path((group_id, local_id)): Path<(i64, i64)>,
    body: Proto<pb::WriteComment>,
) -> Result<impl IntoResponse, AppError> {
    write_wall_user(state, auth, Path((-group_id, local_id)), body).await
}

pub async fn list_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, photo_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::CommentList>, AppError> {
    let _photo = media::services::get_photo(&state, owner_id, photo_id).await?;
    let mut comments = services::list(&state, CommentTarget::Photo, owner_id, photo_id).await?;
    crate::modules::likes::services::attach_comments(&state, &mut comments, auth.user_id).await?;
    Ok(Proto(codec::comments_to_pb(comments)))
}

pub async fn write_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, photo_id)): Path<(i64, i64)>,
    Proto(body): Proto<pb::WriteComment>,
) -> Result<impl IntoResponse, AppError> {
    let photo = media::services::get_photo(&state, owner_id, photo_id).await?;
    let comment = services::write(
        &state,
        auth.user_id,
        CommentTarget::Photo,
        owner_id,
        photo_id,
        body.content,
        Some(photo.owner_user_id),
        &format!("/photo{owner_id}_{photo_id}"),
    )
    .await?;
    Ok((StatusCode::CREATED, Proto(codec::comment_to_pb(&comment))))
}

pub async fn list_video(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, video_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::CommentList>, AppError> {
    let _video = media::services::get_video(&state, owner_id, video_id).await?;
    let mut comments = services::list(&state, CommentTarget::Video, owner_id, video_id).await?;
    crate::modules::likes::services::attach_comments(&state, &mut comments, auth.user_id).await?;
    Ok(Proto(codec::comments_to_pb(comments)))
}

pub async fn write_video(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, video_id)): Path<(i64, i64)>,
    Proto(body): Proto<pb::WriteComment>,
) -> Result<impl IntoResponse, AppError> {
    let video = media::services::get_video(&state, owner_id, video_id).await?;
    let comment = services::write(
        &state,
        auth.user_id,
        CommentTarget::Video,
        owner_id,
        video_id,
        body.content,
        Some(video.owner_user_id),
        &format!("/video{owner_id}_{video_id}"),
    )
    .await?;
    Ok((StatusCode::CREATED, Proto(codec::comment_to_pb(&comment))))
}
