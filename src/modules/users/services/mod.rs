use crate::cache;
use crate::error::AppError;
use crate::modules::media::services as media_services;
use crate::modules::users::models::{ChangePassword, DeleteAccount, UpdateAccount, User};
use crate::state::AppState;

pub async fn get_profile(state: &AppState, id: i64) -> Result<User, AppError> {
    if let Some(cached) = state.caches.users.get(&id).await {
        return Ok(cached);
    }

    if let Some(cached) = cache::get_cached_user(&state.redis, &state.vault, id).await {
        state.caches.users.insert(id, cached.clone()).await;
        return Ok(cached);
    }

    let user = state
        .users()
        .find_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    state.caches.users.insert(id, user.clone()).await;
    cache::cache_user(&state.redis, &state.vault, id, &user).await;
    Ok(user)
}

pub async fn update_account(
    state: &AppState,
    id: i64,
    patch: UpdateAccount,
) -> Result<User, AppError> {
    if patch.first_name.trim().is_empty() || patch.last_name.trim().is_empty() {
        return Err(AppError::Validation("name and surname are required".into()));
    }

    let user = state.users().update_account(id, &patch).await?;
    state.caches.users.insert(id, user.clone()).await;
    cache::invalidate_user(&state.redis, id).await;
    cache::cache_user(&state.redis, &state.vault, id, &user).await;
    Ok(user)
}

pub async fn change_password(
    state: &AppState,
    id: i64,
    body: ChangePassword,
) -> Result<(), AppError> {
    let fields = state
        .security
        .open_sealed(&body.challenge_id, &body.password_sealed, 2)
        .await?;
    let current_password = fields.first().cloned().ok_or_else(|| {
        AppError::Validation("password must be sealed with the login challenge".into())
    })?;
    let new_password = fields.get(1).cloned().ok_or_else(|| {
        AppError::Validation("password must be sealed with the login challenge".into())
    })?;
    if new_password.len() < 8 {
        return Err(AppError::Validation(
            "new password must be at least 8 characters".into(),
        ));
    }
    state
        .users()
        .change_password(id, &current_password, &new_password)
        .await?;
    Ok(())
}

pub async fn replace_cached_user(state: &AppState, user: User) {
    state.caches.users.insert(user.id, user.clone()).await;
    cache::invalidate_user(&state.redis, user.id).await;
    cache::cache_user(&state.redis, &state.vault, user.id, &user).await;
}

pub async fn reload(state: &AppState, id: i64) -> Result<User, AppError> {
    state.caches.users.invalidate(&id).await;
    cache::invalidate_user(&state.redis, id).await;
    get_profile(state, id).await
}

pub async fn delete_account(
    state: &AppState,
    id: i64,
    body: DeleteAccount,
) -> Result<(), AppError> {
    let fields = state
        .security
        .open_sealed(&body.challenge_id, &body.password_sealed, 1)
        .await?;
    let password = fields.first().cloned().ok_or_else(|| {
        AppError::Validation("password must be sealed with the login challenge".into())
    })?;
    if password.is_empty() {
        return Err(AppError::Validation("password is required".into()));
    }
    let users = state.users();
    users.verify_password(id, &password).await?;
    media_services::delete_all_user_objects(state, id).await?;
    users.delete_account(id).await?;
    state.caches.users.invalidate(&id).await;
    state.caches.friend_ids.invalidate(&id).await;
    cache::invalidate_user(&state.redis, id).await;
    Ok(())
}
