use axum::extract::State;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::modules::about::repository::AboutRepository;
use crate::pb;
use crate::state::AppState;

/// AboutInstance.latte
pub async fn instance(State(state): State<AppState>) -> Result<Proto<pb::InstanceAbout>, AppError> {
    let about = AboutRepository::new(&state.db).instance().await?;
    Ok(Proto(codec::instance_about_to_pb(&about)))
}
