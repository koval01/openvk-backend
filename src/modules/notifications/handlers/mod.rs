use axum::extract::State;
use axum::http::StatusCode;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::notifications::services;
use crate::pb;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::NotificationList>, AppError> {
    Ok(Proto(codec::notifications_to_pb(
        services::list(&state, auth.user_id).await?,
    )))
}

pub async fn mark_seen(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    services::mark_seen(&state, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
