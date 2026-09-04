use axum::extract::{Path, State};

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::likes::models::{LikeKind, LikeTarget};
use crate::modules::likes::services;
use crate::pb;
use crate::state::AppState;

pub async fn wall_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    toggle(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Wall,
            owner_id,
            object_id: local_id,
        },
    )
    .await
}

pub async fn wall_group(
    state: State<AppState>,
    auth: AuthUser,
    Path((group_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    wall_user(state, auth, Path((-group_id, local_id))).await
}

pub async fn get_wall_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    state_of(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Wall,
            owner_id,
            object_id: local_id,
        },
    )
    .await
}

pub async fn get_wall_group(
    state: State<AppState>,
    auth: AuthUser,
    Path((group_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    get_wall_user(state, auth, Path((-group_id, local_id))).await
}

pub async fn wall_likers_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::UserList>, AppError> {
    likers(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Wall,
            owner_id,
            object_id: local_id,
        },
    )
    .await
}

pub async fn wall_likers_group(
    state: State<AppState>,
    auth: AuthUser,
    Path((group_id, local_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::UserList>, AppError> {
    wall_likers_user(state, auth, Path((-group_id, local_id))).await
}

pub async fn photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, photo_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    toggle(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Photo,
            owner_id,
            object_id: photo_id,
        },
    )
    .await
}

pub async fn get_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, photo_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    state_of(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Photo,
            owner_id,
            object_id: photo_id,
        },
    )
    .await
}

pub async fn photo_likers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, photo_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::UserList>, AppError> {
    likers(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Photo,
            owner_id,
            object_id: photo_id,
        },
    )
    .await
}

pub async fn video(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, video_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    toggle(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Video,
            owner_id,
            object_id: video_id,
        },
    )
    .await
}

pub async fn get_video(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, video_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::LikeState>, AppError> {
    state_of(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Video,
            owner_id,
            object_id: video_id,
        },
    )
    .await
}

pub async fn video_likers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((owner_id, video_id)): Path<(i64, i64)>,
) -> Result<Proto<pb::UserList>, AppError> {
    likers(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Video,
            owner_id,
            object_id: video_id,
        },
    )
    .await
}

pub async fn comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(comment_id): Path<i64>,
) -> Result<Proto<pb::LikeState>, AppError> {
    toggle(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Comment,
            owner_id: 0,
            object_id: comment_id,
        },
    )
    .await
}

pub async fn get_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(comment_id): Path<i64>,
) -> Result<Proto<pb::LikeState>, AppError> {
    state_of(
        &state,
        auth.user_id,
        LikeTarget {
            kind: LikeKind::Comment,
            owner_id: 0,
            object_id: comment_id,
        },
    )
    .await
}

async fn toggle(
    state: &AppState,
    origin: i64,
    target: LikeTarget,
) -> Result<Proto<pb::LikeState>, AppError> {
    Ok(Proto(codec::like_state_to_pb(
        services::toggle(state, origin, target).await?,
    )))
}

async fn state_of(
    state: &AppState,
    origin: i64,
    target: LikeTarget,
) -> Result<Proto<pb::LikeState>, AppError> {
    Ok(Proto(codec::like_state_to_pb(
        services::state(state, origin, target).await?,
    )))
}

async fn likers(
    state: &AppState,
    _origin: i64,
    target: LikeTarget,
) -> Result<Proto<pb::UserList>, AppError> {
    Ok(Proto(codec::users_to_pb(
        services::likers(state, target).await?,
    )))
}
