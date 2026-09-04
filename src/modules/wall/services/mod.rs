use crate::error::AppError;
use crate::modules::friends::repository::FriendshipRepository;
use crate::modules::wall::models::WallPost;
use crate::modules::wall::repository::{self, WallPostRepository};
use crate::state::AppState;

pub async fn news(state: &AppState, viewer_id: i64) -> Result<Vec<WallPost>, AppError> {
    let mut targets = FriendshipRepository::new(&state.db)
        .friend_ids(viewer_id)
        .await?;
    targets.push(viewer_id);
    let posts = WallPostRepository::new(&state.db, &state.vault)
        .list_news(&targets)
        .await?;
    cache_posts(state, &posts).await;
    Ok(posts)
}

pub async fn list_wall(state: &AppState, target_id: i64) -> Result<Vec<WallPost>, AppError> {
    let posts = WallPostRepository::new(&state.db, &state.vault)
        .list_for_target(target_id)
        .await?;
    cache_posts(state, &posts).await;
    Ok(posts)
}

pub async fn write_wall(
    state: &AppState,
    author_id: i64,
    target_id: i64,
    content: String,
) -> Result<WallPost, AppError> {
    let content = content.trim().to_owned();
    if content.is_empty() {
        return Err(AppError::Validation("wall entry cannot be empty".into()));
    }
    if content.chars().count() > 4096 {
        return Err(AppError::Validation(
            "wall entry is longer than 4096 characters".into(),
        ));
    }

    let users = state.users();
    let target = users
        .find_by_id(target_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let author = users
        .find_by_id(author_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let is_friend = FriendshipRepository::new(&state.db)
        .is_friend(author_id, target_id)
        .await?;
    if !repository::can_write_wall(author_id, &target, is_friend) {
        return Err(AppError::Forbidden);
    }

    let post = WallPostRepository::new(&state.db, &state.vault)
        .insert(&target, &author, content)
        .await?;
    state
        .caches
        .posts
        .insert((post.target_id, post.id), post.clone())
        .await;
    Ok(post)
}

pub async fn get_wall_post(
    state: &AppState,
    target_id: i64,
    local_id: i64,
) -> Result<WallPost, AppError> {
    if let Some(cached) = state.caches.posts.get(&(target_id, local_id)).await {
        return Ok(cached);
    }
    let post = WallPostRepository::new(&state.db, &state.vault)
        .get_for_target(target_id, local_id)
        .await?
        .ok_or(AppError::NotFound)?;
    state
        .caches
        .posts
        .insert((post.target_id, post.id), post.clone())
        .await;
    Ok(post)
}

async fn cache_posts(state: &AppState, posts: &[WallPost]) {
    for post in posts {
        state
            .caches
            .posts
            .insert((post.target_id, post.id), post.clone())
            .await;
    }
}
