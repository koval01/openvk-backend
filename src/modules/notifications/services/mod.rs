use crate::error::AppError;
use crate::modules::notifications::models::Notification;
use crate::modules::notifications::repository::NotificationRepository;
use crate::state::AppState;

pub async fn list(state: &AppState, user_id: i64) -> Result<Vec<Notification>, AppError> {
    NotificationRepository::new(&state.db)
        .list_for_user(user_id, &state.vault, state.media_base_url())
        .await
}

pub async fn insert_comment(
    state: &AppState,
    user_id: i64,
    actor_id: i64,
    entity_type: &str,
    entity_id: i64,
    href: &str,
) -> Result<(), AppError> {
    NotificationRepository::new(&state.db)
        .insert_comment(user_id, actor_id, entity_type, entity_id, href)
        .await
}

pub async fn insert_like(
    state: &AppState,
    user_id: i64,
    actor_id: i64,
    entity_type: &str,
    entity_id: i64,
    href: &str,
) -> Result<(), AppError> {
    NotificationRepository::new(&state.db)
        .insert_like(user_id, actor_id, entity_type, entity_id, href)
        .await
}

pub async fn mark_seen(state: &AppState, user_id: i64) -> Result<(), AppError> {
    NotificationRepository::new(&state.db)
        .mark_seen(user_id)
        .await
}
