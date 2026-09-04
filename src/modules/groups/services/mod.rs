use crate::error::AppError;
use crate::modules::groups::models::Group;
use crate::modules::groups::repository::GroupRepository;
use crate::state::AppState;

pub async fn list(state: &AppState) -> Result<Vec<Group>, AppError> {
    GroupRepository::new(&state.db, state.media_base_url())
        .list()
        .await
}

pub async fn get(state: &AppState, id: i64) -> Result<Group, AppError> {
    GroupRepository::new(&state.db, state.media_base_url())
        .get(id)
        .await?
        .ok_or(AppError::NotFound)
}
