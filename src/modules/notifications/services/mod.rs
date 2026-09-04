use crate::error::AppError;
use crate::modules::notifications::models::Notification;
use crate::modules::notifications::repository::NotificationRepository;
use crate::state::AppState;

pub async fn list(state: &AppState, user_id: i64) -> Result<Vec<Notification>, AppError> {
    NotificationRepository::new(&state.db)
        .list_for_user(user_id)
        .await
}
