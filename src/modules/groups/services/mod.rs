use crate::error::AppError;
use crate::modules::groups::models::Group;
use crate::modules::groups::repository::GroupRepository;
use crate::state::AppState;

pub async fn list(state: &AppState) -> Result<Vec<Group>, AppError> {
    GroupRepository::new(&state.db).list().await
}
