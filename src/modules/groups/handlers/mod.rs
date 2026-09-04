use axum::extract::{Path, State};

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::groups::services;
use crate::pb;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Proto<pb::GroupList>, AppError> {
    Ok(Proto(codec::groups_to_pb(services::list(&state).await?)))
}

pub async fn get(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::Group>, AppError> {
    Ok(Proto(codec::group_to_pb(&services::get(&state, id).await?)))
}
