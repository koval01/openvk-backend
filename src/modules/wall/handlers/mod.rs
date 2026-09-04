use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::wall::services;
use crate::pb;
use crate::state::AppState;

pub async fn list_wall(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::WallPostList>, AppError> {
    let mut posts = services::list_wall(&state, id).await?;
    crate::modules::likes::services::attach_wall(&state, &mut posts, auth.user_id).await?;
    Ok(Proto(codec::wall_posts_to_pb(posts)))
}

pub async fn write_wall(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::WriteWall>,
) -> Result<impl IntoResponse, AppError> {
    let post = services::write_wall(&state, auth.user_id, id, body).await?;
    Ok((StatusCode::CREATED, Proto(codec::wall_post_to_pb(&post))))
}

pub async fn get_wall_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::WallPost>, AppError> {
    let mut post = services::get_wall_post(&state, id, local_id).await?;
    crate::modules::likes::services::attach_wall(
        &state,
        std::slice::from_mut(&mut post),
        auth.user_id,
    )
    .await?;
    Ok(Proto(codec::wall_post_to_pb(&post)))
}

pub async fn list_group_wall(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::WallPostList>, AppError> {
    list_wall(State(state), auth, Path(-id)).await
}

pub async fn write_group_wall(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    body: Proto<pb::WriteWall>,
) -> Result<impl IntoResponse, AppError> {
    write_wall(State(state), auth, Path(-id), body).await
}

pub async fn get_group_wall_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::WallPost>, AppError> {
    get_wall_post(State(state), auth, Path((-id, local_id))).await
}
