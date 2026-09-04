use crate::cache;
use crate::error::AppError;
use crate::modules::friends::repository::FriendshipRepository;
use crate::modules::users::User;
use crate::state::AppState;

pub async fn list_friends(state: &AppState, user_id: i64) -> Result<Vec<User>, AppError> {
    let ids = friend_ids(state, user_id).await?;
    state.users().find_many(&ids).await
}

pub async fn friend_ids(state: &AppState, user_id: i64) -> Result<Vec<i64>, AppError> {
    if let Some(cached) = state.caches.friend_ids.get(&user_id).await {
        return Ok(cached);
    }

    if let Some(cached) =
        cache::get_json::<Vec<i64>>(&state.redis, &cache::friends_key(user_id)).await
    {
        state
            .caches
            .friend_ids
            .insert(user_id, cached.clone())
            .await;
        return Ok(cached);
    }

    let ids = FriendshipRepository::new(&state.db)
        .friend_ids(user_id)
        .await?;
    state.caches.friend_ids.insert(user_id, ids.clone()).await;
    cache::cache_friends(&state.redis, user_id, &ids).await;
    Ok(ids)
}
