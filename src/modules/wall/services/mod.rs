use crate::error::AppError;
use crate::modules::friends::repository::FriendshipRepository;
use crate::modules::groups::repository::GroupRepository;
use crate::modules::media;
use crate::modules::wall::models::{GeoPoint, WallAttachment, WallPost, WriteWall};
use crate::modules::wall::repository::{self, WallPostRepository};
use crate::pb;
use crate::state::AppState;

const MAX_ATTACHMENTS: usize = 10;

pub async fn news(state: &AppState, viewer_id: i64) -> Result<Vec<WallPost>, AppError> {
    let mut targets = FriendshipRepository::new(&state.db)
        .friend_ids(viewer_id)
        .await?;
    targets.push(viewer_id);
    let posts = WallPostRepository::new(&state.db, &state.vault, state.media_base_url())
        .list_news(&targets)
        .await?;
    cache_posts(state, &posts).await;
    Ok(posts)
}

pub async fn list_wall(state: &AppState, target_id: i64) -> Result<Vec<WallPost>, AppError> {
    let posts = WallPostRepository::new(&state.db, &state.vault, state.media_base_url())
        .list_for_target(target_id)
        .await?;
    cache_posts(state, &posts).await;
    Ok(posts)
}

pub async fn write_wall(
    state: &AppState,
    author_id: i64,
    target_id: i64,
    body: pb::WriteWall,
) -> Result<WallPost, AppError> {
    let write = parse_write(state, body).await?;
    if target_id < 0 {
        return write_group_wall(state, author_id, -target_id, write).await;
    }
    write_user_wall(state, author_id, target_id, write).await
}

pub async fn write_group_wall(
    state: &AppState,
    author_id: i64,
    group_id: i64,
    write: WriteWall,
) -> Result<WallPost, AppError> {
    let group = GroupRepository::new(&state.db, state.media_base_url())
        .get(group_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let author = state
        .users()
        .find_by_id(author_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !author.posting_allowed && !author.is_agent() {
        return Err(AppError::Forbidden);
    }
    let is_member = GroupRepository::new(&state.db, state.media_base_url())
        .is_active_member(group_id, author_id)
        .await?;
    if !repository::can_write_group_wall(author_id, &group, is_member) {
        return Err(AppError::Forbidden);
    }
    let post = WallPostRepository::new(&state.db, &state.vault, state.media_base_url())
        .insert_group(&group, &author, write)
        .await?;
    state
        .caches
        .posts
        .insert((post.target_id, post.id), post.clone())
        .await;
    Ok(post)
}

async fn write_user_wall(
    state: &AppState,
    author_id: i64,
    target_id: i64,
    write: WriteWall,
) -> Result<WallPost, AppError> {
    let users = state.users();
    let target = users
        .find_by_id(target_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let author = users
        .find_by_id(author_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !author.posting_allowed && !author.is_agent() {
        return Err(AppError::Forbidden);
    }

    let is_friend = FriendshipRepository::new(&state.db)
        .is_friend(author_id, target_id)
        .await?;
    if !repository::can_write_wall(author_id, &target, is_friend) {
        return Err(AppError::Forbidden);
    }

    let post = WallPostRepository::new(&state.db, &state.vault, state.media_base_url())
        .insert_user(&target, &author, write)
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
    let post = WallPostRepository::new(&state.db, &state.vault, state.media_base_url())
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

async fn parse_write(state: &AppState, body: pb::WriteWall) -> Result<WriteWall, AppError> {
    let content = body.content.trim().to_owned();
    if content.chars().count() > 4096 {
        return Err(AppError::Validation(
            "wall entry is longer than 4096 characters".into(),
        ));
    }
    if body.attachments.len() > MAX_ATTACHMENTS {
        return Err(AppError::Validation("too many attachments".into()));
    }
    let mut attachments = Vec::new();
    for item in body.attachments {
        if let Some(attachment) = resolve_attachment(state, item).await? {
            attachments.push(attachment);
        }
    }
    let geo = match body.geo {
        Some(geo) => Some(parse_geo(geo)?),
        None => None,
    };
    let source = body
        .source
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(ref source) = source {
        if source.chars().count() > 400 {
            return Err(AppError::Validation("source is too long".into()));
        }
    }
    if content.is_empty() && attachments.is_empty() && geo.is_none() {
        return Err(AppError::Validation("wall entry cannot be empty".into()));
    }
    Ok(WriteWall {
        content,
        attachments,
        geo,
        source,
        nsfw: body.nsfw,
    })
}

fn parse_geo(geo: pb::GeoPoint) -> Result<GeoPoint, AppError> {
    if !(-90.0..=90.0).contains(&geo.lat) || !(-180.0..=180.0).contains(&geo.lng) {
        return Err(AppError::Validation("invalid map coordinates".into()));
    }
    let name = geo.name.trim();
    if name.chars().count() > 200 {
        return Err(AppError::Validation("place name is too long".into()));
    }
    Ok(GeoPoint {
        lat: geo.lat,
        lng: geo.lng,
        name: if name.is_empty() {
            format!("{:.4}, {:.4}", geo.lat, geo.lng)
        } else {
            name.to_owned()
        },
    })
}

async fn resolve_attachment(
    state: &AppState,
    item: pb::WallAttachment,
) -> Result<Option<WallAttachment>, AppError> {
    let kind = item.kind.trim().to_ascii_lowercase();
    match kind.as_str() {
        "geo" | "source" => Ok(None),
        "photo" => {
            let photo = media::services::get_photo_by_id(state, item.object_id).await?;
            Ok(Some(WallAttachment {
                kind: "photo".into(),
                owner_id: photo.owner_user_id,
                object_id: photo.id,
                url: photo.url,
                title: item.title,
                src: String::new(),
            }))
        }
        "video" => {
            let video = media::services::get_video_by_id(state, item.object_id).await?;
            Ok(Some(WallAttachment {
                kind: "video".into(),
                owner_id: video.owner_user_id,
                object_id: video.id,
                url: video.src.clone().unwrap_or_default(),
                title: if item.title.is_empty() {
                    video.title
                } else {
                    item.title
                },
                src: video.src.unwrap_or_default(),
            }))
        }
        "audio" => {
            let track = media::services::get_audio_by_id(state, item.object_id).await?;
            Ok(Some(WallAttachment {
                kind: "audio".into(),
                owner_id: track.owner_user_id,
                object_id: track.id,
                url: track.src.clone(),
                title: if item.title.is_empty() {
                    format!("{} — {}", track.artist, track.title)
                } else {
                    item.title
                },
                src: track.src,
            }))
        }
        "poll" | "document" | "note" => {
            let title = item.title.trim();
            if title.is_empty() {
                return Err(AppError::Validation(format!("{kind} needs a title")));
            }
            Ok(Some(WallAttachment {
                kind,
                owner_id: 0,
                object_id: 0,
                url: String::new(),
                title: title.to_owned(),
                src: String::new(),
            }))
        }
        _ => Err(AppError::Validation(format!(
            "unsupported attachment kind {kind}"
        ))),
    }
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
