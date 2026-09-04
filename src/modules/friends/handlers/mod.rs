use axum::extract::{Path, State};

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::friends::services;
use crate::pb;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::UserList>, AppError> {
    Ok(Proto(codec::users_to_pb(
        services::list_friends(&state, auth.user_id).await?,
    )))
}

pub async fn list_for_user(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::UserList>, AppError> {
    Ok(Proto(codec::users_to_pb(
        services::list_friends(&state, id).await?,
    )))
}
