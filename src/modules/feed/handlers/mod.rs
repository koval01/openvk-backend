use axum::extract::State;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::feed::services;
use crate::pb;
use crate::state::AppState;

pub async fn news(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Proto<pb::WallPostList>, AppError> {
    Ok(Proto(codec::wall_posts_to_pb({
        let mut posts = services::chronological(&state, auth.user_id).await?;
        crate::modules::likes::services::attach_wall(&state, &mut posts, auth.user_id).await?;
        posts
    })))
}
