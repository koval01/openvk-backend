use crate::error::AppError;
use crate::modules::comments::models::{Comment, CommentTarget};
use crate::modules::comments::repository::CommentRepository;
use crate::modules::notifications::services as notifications;
use crate::state::AppState;

pub async fn list(
    state: &AppState,
    target: CommentTarget,
    owner_id: i64,
    object_id: i64,
) -> Result<Vec<Comment>, AppError> {
    CommentRepository::new(&state.db, &state.vault, state.media_base_url())
        .list(target, owner_id, object_id)
        .await
}

pub async fn write(
    state: &AppState,
    author_id: i64,
    target: CommentTarget,
    owner_id: i64,
    object_id: i64,
    content: String,
    notify_user_id: Option<i64>,
    href: &str,
) -> Result<Comment, AppError> {
    let content = content.trim().to_owned();
    if content.is_empty() {
        return Err(AppError::Validation("comment cannot be empty".into()));
    }
    if content.chars().count() > 4096 {
        return Err(AppError::Validation(
            "comment is longer than 4096 characters".into(),
        ));
    }
    let author = state
        .users()
        .find_by_id(author_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let comment = CommentRepository::new(&state.db, &state.vault, state.media_base_url())
        .insert(target, owner_id, object_id, &author, content)
        .await?;
    if target == CommentTarget::Wall {
        state.caches.posts.invalidate(&(owner_id, object_id)).await;
    }
    if let Some(user_id) = notify_user_id.filter(|id| *id != author_id) {
        notifications::insert_comment(state, user_id, author_id, target.as_str(), object_id, href)
            .await?;
    }
    Ok(comment)
}
