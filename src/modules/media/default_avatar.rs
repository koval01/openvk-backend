use std::time::Duration;

use bytes::Bytes;
use image::ExtendedColorType;
use image::codecs::webp::WebPEncoder;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, Tree};

use crate::config::DICEBEAR_DEFAULT_URL;
use crate::error::AppError;
use crate::modules::media::kinds::MediaKind;
use crate::modules::media::repository::MediaRepository;
use crate::modules::media::services;
use crate::modules::users;
use crate::state::AppState;

/// Notionists SVG with the greys from the product embed. `{seed}` is the user id.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_URL: &str = DICEBEAR_DEFAULT_URL;

const MAX_BYTES: usize = 512 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Classic VK `photo_200` edge. `DiceBear` Notionists ships a 1744 viewBox.
const AVATAR_EDGE: u32 = 200;
const AVATAR_EDGE_F: f32 = 200.0;

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
    let svg_keys = state.users().svg_avatar_keys().await?;
    if !svg_keys.is_empty() {
        tracing::info!(count = svg_keys.len(), "converting svg avatars to webp");
    }
    for (user_id, key) in svg_keys {
        if let Err(error) = convert_svg_avatar(state, user_id, &key).await {
            tracing::warn!(user_id, %error, "svg avatar not converted");
        }
    }
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

async fn convert_svg_avatar(state: &AppState, user_id: i64, key: &str) -> Result<(), AppError> {
    let stored = state.storage.get(key).await?;
    if !is_safe_svg(&stored.bytes) {
        return Err(AppError::Validation(
            "stored avatar is not a safe svg".into(),
        ));
    }
    persist(state, user_id, stored.bytes).await?;
    if state.storage.exists(key).await.unwrap_or(false) {
        let _ = state.storage.delete(key).await;
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
    let repo = MediaRepository::new(&state.db, &state.config.media_public_base_url);
    let previous = repo.find_avatar(user_id).await?;
    let upload = if is_safe_svg(&bytes) {
        let webp = tokio::task::spawn_blocking(move || svg_to_webp(&bytes))
            .await
            .map_err(|error| AppError::internal(format!("avatar raster cancelled: {error}")))??;
        services::PreparedUpload {
            file_name: "avatar.webp".into(),
            mime: "image/webp".into(),
            bytes: webp,
            title: None,
            artist: None,
            description: None,
        }
    } else {
        services::PreparedUpload {
            file_name: "avatar.png".into(),
            mime: "image/png".into(),
            bytes,
            title: None,
            artist: None,
            description: None,
        }
    };
    let media = services::persist_object(state, user_id, MediaKind::Avatar, &upload).await?;
    let user = state
        .users()
        .set_avatar_key(user_id, Some(media.storage_key.clone()))
        .await?;
    users::services::replace_cached_user(state, user).await;
    if let Some(previous) = previous {
        if previous.storage_key != media.storage_key {
            let _ = state.storage.delete(&previous.storage_key).await;
            let _ = repo.delete_media_row(previous.id).await;
        }
    }
    Ok(())
}

fn svg_to_webp(svg: &[u8]) -> Result<Bytes, AppError> {
    let opt = usvg::Options {
        default_size: usvg::Size::from_wh(AVATAR_EDGE_F, AVATAR_EDGE_F)
            .ok_or_else(|| AppError::internal("avatar raster size"))?,
        ..usvg::Options::default()
    };
    let tree = Tree::from_data(svg, &opt)
        .map_err(|error| AppError::Validation(format!("dicebear svg: {error}")))?;
    let tree_size = tree.size();
    let scale = (AVATAR_EDGE_F / tree_size.width()).min(AVATAR_EDGE_F / tree_size.height());
    let mut pixmap =
        Pixmap::new(AVATAR_EDGE, AVATAR_EDGE).ok_or_else(|| AppError::internal("avatar pixmap"))?;
    let tx = (AVATAR_EDGE_F - tree_size.width() * scale) * 0.5;
    let ty = (AVATAR_EDGE_F - tree_size.height() * scale) * 0.5;
    resvg::render(
        &tree,
        Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
        &mut pixmap.as_mut(),
    );
    encode_lossless_webp(&unpremultiply(&pixmap), AVATAR_EDGE, AVATAR_EDGE)
}

fn unpremultiply(pixmap: &Pixmap) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    rgba
}

fn encode_lossless_webp(rgba: &[u8], width: u32, height: u32) -> Result<Bytes, AppError> {
    let mut out = Vec::new();
    WebPEncoder::new_lossless(&mut out)
        .encode(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| AppError::Validation(format!("dicebear webp: {error}")))?;
    if out.len() < 12 || !out.starts_with(b"RIFF") || out.get(8..12) != Some(b"WEBP") {
        return Err(AppError::internal("webp encoder produced invalid output"));
    }
    Ok(Bytes::from(out))
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
    use super::{DEFAULT_URL, is_safe_svg, svg_to_webp, url_for_user};

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

    #[test]
    fn svg_rasterizes_to_a_webp_square() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#cc0000"/></svg>"##;
        let webp = svg_to_webp(svg).expect("rasterize");
        assert!(webp.starts_with(b"RIFF"));
        assert_eq!(&webp[8..12], b"WEBP");
        let img = image::load_from_memory(&webp).expect("decode webp");
        assert_eq!(img.width(), 200);
        assert_eq!(img.height(), 200);
        let pixel = img.to_rgba8().get_pixel(100, 100).0;
        assert!(pixel[0] > 160, "{pixel:?}");
        assert!(pixel[1] < 40, "{pixel:?}");
        assert!(pixel[2] < 40, "{pixel:?}");
    }

    #[test]
    fn svg_defs_and_use_rasterize() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><rect id="r" width="10" height="10" fill="#00aa00"/></defs><use href="#r"/></svg>"##;
        let webp = svg_to_webp(svg).expect("rasterize use");
        let pixel = image::load_from_memory(&webp)
            .expect("decode")
            .to_rgba8()
            .get_pixel(100, 100)
            .0;
        assert!(pixel[1] > 120, "{pixel:?}");
    }
}
