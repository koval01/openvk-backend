use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::users::models::{ChangePassword, DeleteAccount};
use crate::modules::users::services;
use crate::pb;
use crate::state::AppState;

pub async fn get_user(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::get_profile(&state, id).await?,
    )))
}

pub async fn get_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::get_profile(&state, auth.user_id).await?,
    )))
}

pub async fn update_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::UpdateAccount>,
) -> Result<Proto<pb::User>, AppError> {
    Ok(Proto(codec::user_to_pb(
        &services::update_account(&state, auth.user_id, codec::update_account_from_pb(body))
            .await?,
    )))
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::SealedPassword>,
) -> Result<StatusCode, AppError> {
    services::change_password(
        &state,
        auth.user_id,
        ChangePassword {
            challenge_id: body.challenge_id,
            password_sealed: body.password_sealed,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::SealedPassword>,
) -> Result<StatusCode, AppError> {
    services::delete_account(
        &state,
        auth.user_id,
        DeleteAccount {
            challenge_id: body.challenge_id,
            password_sealed: body.password_sealed,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
