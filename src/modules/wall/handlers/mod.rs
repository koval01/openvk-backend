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
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::WallPostList>, AppError> {
    Ok(Proto(codec::wall_posts_to_pb(
        services::list_wall(&state, id).await?,
    )))
}

pub async fn write_wall(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Proto(body): Proto<pb::WriteWall>,
) -> Result<impl IntoResponse, AppError> {
    let post = services::write_wall(&state, auth.user_id, id, body.content).await?;
    Ok((StatusCode::CREATED, Proto(codec::wall_post_to_pb(&post))))
}

pub async fn get_wall_post(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::WallPost>, AppError> {
    Ok(Proto(codec::wall_post_to_pb(
        &services::get_wall_post(&state, id, local_id).await?,
    )))
}
