use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING};

use crate::config::DICEBEAR_DEFAULT_URL;
use crate::error::AppError;
use crate::modules::media::kinds::{MediaKind, storage_key};
use crate::modules::media::repository::{MediaRepository, NewMedia};
use crate::modules::media::services;
use crate::modules::users;
use crate::state::AppState;

/// Notionists SVG with the greys from the product embed. `{seed}` is the user id.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_URL: &str = DICEBEAR_DEFAULT_URL;

const MAX_BYTES: usize = 512 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

#[must_use]
pub fn url_for_user(template: &str, user_id: i64) -> String {
    if template.contains("{seed}") {
        return template.replace("{seed}", &user_id.to_string());
    }
    let join = if template.contains('?') { '&' } else { '?' };
    format!("{template}{join}seed={user_id}")
}

pub async fn assign(state: &AppState, user_id: i64) -> Result<(), AppError> {
    let Some(template) = state.config.dicebear_url.as_deref() else {
        return Ok(());
    };
    let Some(user) = state.users().find_by_id(user_id).await? else {
        return Err(AppError::NotFound);
    };
    if user.avatar_url.is_some() {
        return Ok(());
    }
    let url = url_for_user(template, user_id);
    let bytes = fetch_bytes(&url).await?;
    persist(state, user_id, bytes).await?;
    Ok(())
}

pub async fn backfill(state: &AppState) -> Result<(), AppError> {
    if state.config.dicebear_url.is_none() {
        return Ok(());
    }
    let ids = state.users().ids_without_avatar().await?;
    if !ids.is_empty() {
        tracing::info!(count = ids.len(), "storing default avatars");
    }
    for user_id in ids {
        if let Err(error) = assign(state, user_id).await {
            tracing::warn!(user_id, %error, "default avatar not stored");
        }
    }
    Ok(())
}

async fn fetch_bytes(url: &str) -> Result<Bytes, AppError> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent("openvk-backend")
        .build()
        .map_err(|error| AppError::Validation(format!("dicebear client: {error}")))?;
    let response = client
        .get(url)
        .header(ACCEPT, "image/svg+xml,image/*;q=0.9")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| AppError::Validation(format!("dicebear fetch: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Validation(format!("dicebear HTTP {status}")));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Validation(format!("dicebear body: {error}")))?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::Validation("dicebear payload is too large".into()));
    }
    Ok(bytes)
}

async fn persist(state: &AppState, user_id: i64, bytes: Bytes) -> Result<(), AppError> {
    if is_safe_svg(&bytes) {
        return persist_svg(state, user_id, bytes).await;
    }
    let media = services::persist_object(
        state,
        user_id,
        MediaKind::Avatar,
        &services::PreparedUpload {
            file_name: "avatar.png".into(),
            mime: "image/png".into(),
            bytes,
            title: None,
            artist: None,
            description: None,
        },
    )
    .await?;
    let user = state
        .users()
        .set_avatar_key(user_id, Some(media.storage_key))
        .await?;
    users::services::replace_cached_user(state, user).await;
    Ok(())
}

async fn persist_svg(state: &AppState, user_id: i64, bytes: Bytes) -> Result<(), AppError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    MediaKind::Avatar.validate_size(size)?;
    let key = storage_key(user_id, MediaKind::Avatar, "svg");
    state
        .storage
        .put(&key, bytes.clone(), "image/svg+xml")
        .await?;
    let size_bytes = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    let inserted = MediaRepository::new(&state.db, &state.config.media_public_base_url)
        .insert_media(NewMedia {
            owner_user_id: user_id,
            kind: MediaKind::Avatar,
            storage_key: key.clone(),
            mime: "image/svg+xml".into(),
            size_bytes,
            width: None,
            height: None,
            original_filename: Some("avatar.svg".into()),
        })
        .await;
    let media = match inserted {
        Ok(media) => media,
        Err(error) => {
            let _ = state.storage.delete(&key).await;
            return Err(error);
        }
    };
    let user = state
        .users()
        .set_avatar_key(user_id, Some(media.storage_key))
        .await?;
    users::services::replace_cached_user(state, user).await;
    Ok(())
}

fn is_safe_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let looks_like_svg = starts_with_ignore_ascii_case(trimmed, "<svg")
        || (starts_with_ignore_ascii_case(trimmed, "<?xml")
            && contains_ignore_ascii_case(bytes, b"<svg"));
    looks_like_svg
        && !contains_ignore_ascii_case(bytes, b"<script")
        && !contains_ignore_ascii_case(bytes, b"javascript:")
}

fn starts_with_ignore_ascii_case(haystack: &str, prefix: &str) -> bool {
    haystack.len() >= prefix.len()
        && haystack.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_URL, is_safe_svg, url_for_user};

    #[test]
    fn seed_replaces_the_placeholder() {
        assert_eq!(
            url_for_user(DEFAULT_URL, 12_345),
            "https://api.dicebear.com/10.x/notionists/svg?backgroundColor=ececed&inkColor=3b3d42&paperColor=fafafa&seed=12345"
        );
        assert_eq!(
            url_for_user("https://avatars.test/png", 9),
            "https://avatars.test/png?seed=9"
        );
    }

    #[test]
    fn svg_without_script_is_accepted() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        assert!(is_safe_svg(svg));
        assert!(!is_safe_svg(b"<svg><script>alert(1)</script></svg>"));
        assert!(!is_safe_svg(&[0x89, b'P', b'N', b'G']));
    }
}
