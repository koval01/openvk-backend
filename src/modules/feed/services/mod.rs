use crate::error::AppError;
use crate::modules::feed::models::WallPost;
use crate::modules::wall;
use crate::state::AppState;

pub async fn chronological(state: &AppState, viewer_id: i64) -> Result<Vec<WallPost>, AppError> {
    wall::services::news(state, viewer_id).await
}
