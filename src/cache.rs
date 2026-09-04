use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::modules::users::User;
use crate::vault::{Vault, aad_cache_user};

const USER_TTL_SECS: u64 = 30;
const FRIENDS_TTL_SECS: u64 = 30;

pub fn user_key(id: i64) -> String {
    format!("user:{id}")
}

pub fn friends_key(id: i64) -> String {
    format!("friends:{id}")
}

pub async fn get_json<T: DeserializeOwned>(redis: &ConnectionManager, key: &str) -> Option<T> {
    let raw = get_string(redis, key).await?;
    serde_json::from_str(&raw).ok()
}

pub async fn get_cached_user(redis: &ConnectionManager, vault: &Vault, id: i64) -> Option<User> {
    let raw = get_string(redis, &user_key(id)).await?;
    if !Vault::is_sealed(&raw) {
        return None;
    }
    let json = vault.maybe_decrypt(&aad_cache_user(id), &raw).ok()?;
    serde_json::from_str(&json).ok()
}

pub async fn set_json<T: Serialize>(
    redis: &ConnectionManager,
    key: &str,
    value: &T,
    ttl_secs: u64,
) {
    match serde_json::to_string(value) {
        Ok(payload) => set_string(redis, key, payload, ttl_secs).await,
        Err(error) => tracing::debug!(%error, key, "redis cache serialize skipped"),
    }
}

pub async fn cache_user(redis: &ConnectionManager, vault: &Vault, id: i64, value: &User) {
    match serde_json::to_string(value) {
        Ok(json) => match vault.encrypt_field(&aad_cache_user(id), &json) {
            Ok(sealed) => set_string(redis, &user_key(id), sealed, USER_TTL_SECS).await,
            Err(error) => tracing::debug!(%error, id, "redis user cache seal skipped"),
        },
        Err(error) => tracing::debug!(%error, id, "redis user cache serialize skipped"),
    }
}

pub async fn cache_friends<T: Serialize>(redis: &ConnectionManager, id: i64, value: &T) {
    set_json(redis, &friends_key(id), value, FRIENDS_TTL_SECS).await;
}

pub async fn invalidate_user(redis: &ConnectionManager, id: i64) {
    let mut redis = redis.clone();
    let key = user_key(id);
    if let Err(error) = redis.del::<_, i64>(key).await {
        tracing::debug!(%error, id, "redis user cache invalidate skipped");
    }
}

async fn get_string(redis: &ConnectionManager, key: &str) -> Option<String> {
    let mut redis = redis.clone();
    match redis.get::<_, Option<String>>(key).await {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(%error, key, "redis cache read skipped");
            None
        }
    }
}

async fn set_string(redis: &ConnectionManager, key: &str, payload: String, ttl_secs: u64) {
    let mut redis = redis.clone();
    if let Err(error) = redis
        .set_ex::<_, _, redis::Value>(key, payload, ttl_secs)
        .await
    {
        tracing::debug!(%error, key, "redis cache write skipped");
    }
}
