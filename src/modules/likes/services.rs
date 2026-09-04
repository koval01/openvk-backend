use crate::error::AppError;
use crate::modules::comments::models::Comment;
use crate::modules::comments::repository::CommentRepository;
use crate::modules::likes::models::{LikeKind, LikeState, LikeTarget};
use crate::modules::likes::repository::LikeRepository;
use crate::modules::media;
use crate::modules::media::models::{Photo, Video};
use crate::modules::notifications::services as notifications;
use crate::modules::users::User;
use crate::modules::wall;
use crate::modules::wall::models::WallPost;
use crate::state::AppState;

pub async fn toggle(
    state: &AppState,
    origin: i64,
    target: LikeTarget,
) -> Result<LikeState, AppError> {
    ensure_target(state, target).await?;
    let before = LikeRepository::new(state).state_for(origin, target).await?;
    let after = LikeRepository::new(state).toggle(origin, target).await?;
    if target.kind == LikeKind::Wall {
        state
            .caches
            .posts
            .invalidate(&(target.owner_id, target.object_id))
            .await;
    }
    if after.liked && !before.liked {
        notify(state, origin, target).await?;
    }
    Ok(after)
}

pub async fn set(
    state: &AppState,
    origin: i64,
    target: LikeTarget,
    liked: bool,
) -> Result<LikeState, AppError> {
    ensure_target(state, target).await?;
    let before = LikeRepository::new(state).state_for(origin, target).await?;
    let after = LikeRepository::new(state)
        .set(origin, target, liked)
        .await?;
    if target.kind == LikeKind::Wall {
        state
            .caches
            .posts
            .invalidate(&(target.owner_id, target.object_id))
            .await;
    }
    if after.liked && !before.liked {
        notify(state, origin, target).await?;
    }
    Ok(after)
}

pub async fn state(
    state: &AppState,
    origin: i64,
    target: LikeTarget,
) -> Result<LikeState, AppError> {
    ensure_target(state, target).await?;
    LikeRepository::new(state).state_for(origin, target).await
}

pub async fn likers(state: &AppState, target: LikeTarget) -> Result<Vec<User>, AppError> {
    ensure_target(state, target).await?;
    LikeRepository::new(state).likers(target).await
}

pub async fn attach_wall(
    state: &AppState,
    posts: &mut [WallPost],
    viewer: i64,
) -> Result<(), AppError> {
    let keys: Vec<(i64, i64)> = posts.iter().map(|post| (post.target_id, post.id)).collect();
    let flags = LikeRepository::new(state)
        .flags_for(viewer, LikeKind::Wall.as_str(), &keys)
        .await?;
    for post in posts {
        if let Some(flag) = flags.get(&(post.target_id, post.id)) {
            post.like_count = flag.count;
            post.liked = flag.liked;
        }
    }
    Ok(())
}

pub async fn attach_photos(
    state: &AppState,
    photos: &mut [Photo],
    viewer: i64,
) -> Result<(), AppError> {
    let keys: Vec<(i64, i64)> = photos
        .iter()
        .map(|photo| (photo.owner_user_id, photo.id))
        .collect();
    let flags = LikeRepository::new(state)
        .flags_for(viewer, LikeKind::Photo.as_str(), &keys)
        .await?;
    for photo in photos {
        if let Some(flag) = flags.get(&(photo.owner_user_id, photo.id)) {
            photo.like_count = flag.count;
            photo.liked = flag.liked;
        }
    }
    Ok(())
}

pub async fn attach_videos(
    state: &AppState,
    videos: &mut [Video],
    viewer: i64,
) -> Result<(), AppError> {
    let keys: Vec<(i64, i64)> = videos
        .iter()
        .map(|video| (video.owner_user_id, video.id))
        .collect();
    let flags = LikeRepository::new(state)
        .flags_for(viewer, LikeKind::Video.as_str(), &keys)
        .await?;
    for video in videos {
        if let Some(flag) = flags.get(&(video.owner_user_id, video.id)) {
            video.like_count = flag.count;
            video.liked = flag.liked;
        }
    }
    Ok(())
}

pub async fn attach_comments(
    state: &AppState,
    comments: &mut [Comment],
    viewer: i64,
) -> Result<(), AppError> {
    let keys: Vec<(i64, i64)> = comments.iter().map(|comment| (0, comment.id)).collect();
    let flags = LikeRepository::new(state)
        .flags_for(viewer, LikeKind::Comment.as_str(), &keys)
        .await?;
    for comment in comments {
        if let Some(flag) = flags.get(&(0, comment.id)) {
            comment.like_count = flag.count;
            comment.liked = flag.liked;
        }
    }
    Ok(())
}

async fn ensure_target(state: &AppState, target: LikeTarget) -> Result<(), AppError> {
    match target.kind {
        LikeKind::Wall => {
            let _ = wall::services::get_wall_post(state, target.owner_id, target.object_id).await?;
            Ok(())
        }
        LikeKind::Photo => {
            let _ = media::services::get_photo(state, target.owner_id, target.object_id).await?;
            Ok(())
        }
        LikeKind::Video => {
            let _ = media::services::get_video(state, target.owner_id, target.object_id).await?;
            Ok(())
        }
        LikeKind::Comment => {
            CommentRepository::new(&state.db, &state.vault, state.media_base_url())
                .get(target.object_id)
                .await?
                .ok_or(AppError::NotFound)?;
            Ok(())
        }
    }
}

async fn notify(state: &AppState, origin: i64, target: LikeTarget) -> Result<(), AppError> {
    let (user_id, href) = match target.kind {
        LikeKind::Wall => {
            let post =
                wall::services::get_wall_post(state, target.owner_id, target.object_id).await?;
            (post.author_id, format!("/{}", post.permalink))
        }
        LikeKind::Photo => {
            let photo =
                media::services::get_photo(state, target.owner_id, target.object_id).await?;
            (
                photo.owner_user_id,
                format!("/photo{}_{}", photo.owner_user_id, photo.id),
            )
        }
        LikeKind::Video => {
            let video =
                media::services::get_video(state, target.owner_id, target.object_id).await?;
            (
                video.owner_user_id,
                format!("/video{}_{}", video.owner_user_id, video.id),
            )
        }
        LikeKind::Comment => {
            let comment = CommentRepository::new(&state.db, &state.vault, state.media_base_url())
                .get(target.object_id)
                .await?
                .ok_or(AppError::NotFound)?;
            (comment.author_id, format!("/comment{}", comment.id))
        }
    };
    if user_id == origin {
        return Ok(());
    }
    notifications::insert_like(
        state,
        user_id,
        origin,
        target.kind.as_str(),
        target.object_id,
        &href,
    )
    .await
}
