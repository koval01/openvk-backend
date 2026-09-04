use axum::Json;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Router, extract::FromRequest};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::AppError;
use crate::middleware::auth::token_from_headers;
use crate::modules::auth::services as auth;
use crate::modules::desk;
use crate::modules::friends;
use crate::modules::groups;
use crate::modules::likes;
use crate::modules::likes::models::{LikeKind, LikeTarget};
use crate::modules::media;
use crate::modules::users;
use crate::modules::wall;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/method/{method}", get(call).post(call))
        .route("/method/execute", get(execute).post(execute))
        .route("/token", get(token).post(token))
        .route("/token/", get(token).post(token))
        .route("/oauth/token", get(token).post(token))
}

#[derive(Debug, Default, Deserialize)]
pub struct VkParams {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    user_ids: Option<String>,
    #[serde(default)]
    user_id: Option<i64>,
    #[serde(default)]
    owner_id: Option<i64>,
    #[serde(default)]
    count: Option<i64>,
    #[serde(default)]
    posts: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    grant_type: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    item_id: Option<i64>,
    #[serde(default, rename = "type")]
    object_type: Option<String>,
}

pub async fn call(
    State(state): State<AppState>,
    Path(method): Path<String>,
    headers: HeaderMap,
    Query(query): Query<VkParams>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let params = merge_params(query, request).await;
    match dispatch(&state, &headers, &method, &params).await {
        Ok(value) => (StatusCode::OK, Json(json!({ "response": value }))).into_response(),
        Err(error) => vk_error(error).into_response(),
    }
}

pub async fn execute(
    State(_state): State<AppState>,
    Query(query): Query<VkParams>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let params = merge_params(query, request).await;
    if params
        .code
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        (StatusCode::OK, Json(json!({ "response": 1 }))).into_response()
    } else {
        vk_error(AppError::Validation(
            "execute scripts are not implemented".into(),
        ))
        .into_response()
    }
}

pub async fn token(
    State(state): State<AppState>,
    Query(query): Query<VkParams>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let params = merge_params(query, request).await;
    let login = params.username.unwrap_or_default();
    let password = params.password.unwrap_or_default();
    if login.trim().is_empty() || password.is_empty() {
        return vk_error(AppError::Validation(
            "username and password are required".into(),
        ))
        .into_response();
    }
    match auth::login(&state, &login, &password).await {
        Ok(user_id) => match auth::issue_token(&state, user_id).await {
            Ok(token) => (
                StatusCode::OK,
                Json(json!({
                    "access_token": token,
                    "expires_in": 86400,
                    "user_id": user_id,
                    "token_type": "Bearer"
                })),
            )
                .into_response(),
            Err(error) => vk_error(error).into_response(),
        },
        Err(error) => vk_error(error).into_response(),
    }
}

async fn dispatch(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    params: &VkParams,
) -> Result<Value, AppError> {
    let user_id = current_user(state, headers, params).await?;
    match method {
        "users.get" => users_get(state, user_id, params).await,
        "friends.get" => friends_get(state, user_id, params).await,
        "wall.get" => wall_get(state, user_id, params).await,
        "wall.getById" => wall_get_by_id(state, params).await,
        "photos.get" => photos_get(state, user_id, params).await,
        "audio.get" => audio_get(state, user_id, params).await,
        "video.get" => video_get(state, user_id, params).await,
        "groups.get" => groups_get(state).await,
        "gifts.get" => gifts_get(state, user_id, params).await,
        "account.getInfo" => account_info(state, user_id).await,
        "account.getBalance" => account_balance(state, user_id).await,
        "likes.add" => likes_set(state, user_id, params, true).await,
        "likes.delete" => likes_set(state, user_id, params, false).await,
        "likes.isLiked" => likes_is_liked(state, user_id, params).await,
        "likes.getList" => likes_get_list(state, params).await,
        _ => Err(AppError::NotFound),
    }
}

async fn users_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let ids = parse_ids(params.user_ids.as_deref()).unwrap_or_else(|| vec![viewer]);
    let mut users = Vec::new();
    for id in ids.into_iter().take(100) {
        if let Ok(user) = users::services::get_profile(state, id).await {
            users.push(vk_user(&user));
        }
    }
    Ok(Value::Array(users))
}

async fn friends_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let user_id = params.user_id.unwrap_or(viewer);
    let friends = friends::services::list_friends(state, user_id).await?;
    Ok(json!({
        "count": friends.len(),
        "items": friends.iter().map(|user| user.id).collect::<Vec<_>>(),
    }))
}

async fn wall_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let owner_id = params.owner_id.unwrap_or(viewer);
    let mut posts = wall::services::list_wall(state, owner_id).await?;
    crate::modules::likes::services::attach_wall(state, &mut posts, viewer).await?;
    let count = params.count.unwrap_or(20).clamp(1, 100) as usize;
    let items: Vec<Value> = posts
        .iter()
        .take(count)
        .map(|post| {
            json!({
                "id": post.id,
                "owner_id": post.target_id,
                "from_id": post.author_id,
                "date": post.created_at.timestamp(),
                "text": post.content,
                "post_type": "post",
                "likes": {
                    "count": post.like_count,
                    "user_likes": i32::from(post.liked),
                },
                "comments": { "count": post.comment_count },
            })
        })
        .collect();
    Ok(json!({ "count": posts.len(), "items": items }))
}

async fn wall_get_by_id(state: &AppState, params: &VkParams) -> Result<Value, AppError> {
    let Some(posts) = params.posts.as_deref() else {
        return Err(AppError::Validation("posts is required".into()));
    };
    let mut items = Vec::new();
    for token in posts.split(',').take(20) {
        let Some((owner, local)) = token.split_once('_') else {
            continue;
        };
        let Ok(owner_id) = owner.parse::<i64>() else {
            continue;
        };
        let Ok(local_id) = local.parse::<i64>() else {
            continue;
        };
        if let Ok(post) = wall::services::get_wall_post(state, owner_id, local_id).await {
            items.push(json!({
                "id": post.id,
                "owner_id": post.target_id,
                "from_id": post.author_id,
                "date": post.created_at.timestamp(),
                "text": post.content,
            }));
        }
    }
    Ok(Value::Array(items))
}

async fn photos_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let owner_id = params.owner_id.unwrap_or(viewer);
    let albums = media::services::list_albums(state, Some(owner_id), true).await?;
    let items: Vec<Value> = albums
        .iter()
        .flat_map(|album| album.photos.iter())
        .map(|photo| {
            json!({
                "id": photo.id,
                "album_id": photo.album_id,
                "owner_id": photo.owner_user_id,
                "sizes": [{ "url": photo.url }],
            })
        })
        .collect();
    Ok(json!({ "count": items.len(), "items": items }))
}

async fn audio_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let owner_id = params.owner_id.unwrap_or(viewer);
    let tracks = media::services::list_audio(state, Some(owner_id)).await?;
    Ok(json!({
        "count": tracks.len(),
        "items": tracks.iter().map(|track| json!({
            "id": track.id,
            "owner_id": track.owner_user_id,
            "artist": track.artist,
            "title": track.title,
            "url": track.src,
            "duration": track.duration_ms / 1000,
        })).collect::<Vec<_>>(),
    }))
}

async fn video_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let owner_id = params.owner_id.unwrap_or(viewer);
    let videos = media::services::list_videos(state, Some(owner_id)).await?;
    Ok(json!({
        "count": videos.len(),
        "items": videos.iter().map(|video| json!({
            "id": video.id,
            "owner_id": video.owner_user_id,
            "title": video.title,
            "player": video.src,
        })).collect::<Vec<_>>(),
    }))
}

async fn groups_get(state: &AppState) -> Result<Value, AppError> {
    let groups = groups::services::list(state).await?;
    Ok(json!({
        "count": groups.len(),
        "items": groups.iter().map(|group| json!({
            "id": group.id,
            "name": group.name,
            "screen_name": group.slug,
            "type": "group",
        })).collect::<Vec<_>>(),
    }))
}

async fn gifts_get(state: &AppState, viewer: i64, params: &VkParams) -> Result<Value, AppError> {
    let user_id = params.user_id.unwrap_or(viewer);
    let gifts = desk::services::user_gifts(state, user_id).await?;
    Ok(json!({
        "count": gifts.len(),
        "items": gifts.iter().map(|gift| json!({
            "id": gift.id,
            "from_id": if gift.anonymous { 0 } else { gift.sender_id },
            "message": gift.caption,
            "gift": {
                "id": gift.gift.id,
                "thumb_256": gift.gift.image_url,
            }
        })).collect::<Vec<_>>(),
    }))
}

async fn account_info(state: &AppState, user_id: i64) -> Result<Value, AppError> {
    let user = users::services::get_profile(state, user_id).await?;
    Ok(json!({
        "id": user.id,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "home_town": user.city,
        "status": user.status,
        "2fa_required": 0,
    }))
}

async fn account_balance(state: &AppState, user_id: i64) -> Result<Value, AppError> {
    let user = users::services::get_profile(state, user_id).await?;
    Ok(json!({ "votes": user.coins }))
}

fn vk_user(user: &users::User) -> Value {
    json!({
        "id": user.id,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "screen_name": user.screen_name,
        "photo_200": user.avatar_url,
        "verified": i32::from(user.verified),
        "banned": user.banned,
    })
}

fn vk_like_target(params: &VkParams) -> Result<LikeTarget, AppError> {
    let kind =
        LikeKind::parse(params.object_type.as_deref().unwrap_or("post")).ok_or_else(|| {
            AppError::Validation("type must be post, photo, video, or comment".into())
        })?;
    let object_id = params
        .item_id
        .ok_or_else(|| AppError::Validation("item_id is required".into()))?;
    let owner_id = match kind {
        LikeKind::Comment => 0,
        _ => params
            .owner_id
            .ok_or_else(|| AppError::Validation("owner_id is required".into()))?,
    };
    Ok(LikeTarget {
        kind,
        owner_id,
        object_id,
    })
}

async fn likes_set(
    state: &AppState,
    user_id: i64,
    params: &VkParams,
    liked: bool,
) -> Result<Value, AppError> {
    let target = vk_like_target(params)?;
    let after = likes::services::set(state, user_id, target, liked).await?;
    Ok(json!({ "likes": after.count }))
}

async fn likes_is_liked(
    state: &AppState,
    user_id: i64,
    params: &VkParams,
) -> Result<Value, AppError> {
    let target = vk_like_target(params)?;
    let after = likes::services::state(state, user_id, target).await?;
    Ok(json!({
        "liked": i32::from(after.liked),
        "copied": 0,
    }))
}

async fn likes_get_list(state: &AppState, params: &VkParams) -> Result<Value, AppError> {
    let target = vk_like_target(params)?;
    let users = likes::services::likers(state, target).await?;
    Ok(json!({
        "count": users.len(),
        "items": users.iter().map(|user| user.id).collect::<Vec<_>>(),
    }))
}

async fn current_user(
    state: &AppState,
    headers: &HeaderMap,
    params: &VkParams,
) -> Result<i64, AppError> {
    if let Some(token) = params
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return auth::verify_token(state, token).await;
    }
    let token = token_from_headers(headers).ok_or(AppError::Unauthorized)?;
    auth::verify_token(state, &token).await
}

fn parse_ids(value: Option<&str>) -> Option<Vec<i64>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    Some(
        value
            .split(',')
            .filter_map(|part| part.trim().parse::<i64>().ok())
            .collect(),
    )
}

async fn merge_params(query: VkParams, request: axum::extract::Request) -> VkParams {
    let (mut parts, body) = request.into_parts();
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        parts
            .headers
            .get(axum::http::header::CONTENT_TYPE)
            .cloned()
            .unwrap_or_else(|| {
                axum::http::HeaderValue::from_static("application/x-www-form-urlencoded")
            }),
    );
    let request = axum::extract::Request::from_parts(parts, body);
    if let Ok(Form(form)) = Form::<VkParams>::from_request(request, &()).await {
        merge(query, form)
    } else {
        query
    }
}

fn merge(mut base: VkParams, extra: VkParams) -> VkParams {
    if extra.access_token.is_some() {
        base.access_token = extra.access_token;
    }
    if extra.user_ids.is_some() {
        base.user_ids = extra.user_ids;
    }
    if extra.user_id.is_some() {
        base.user_id = extra.user_id;
    }
    if extra.owner_id.is_some() {
        base.owner_id = extra.owner_id;
    }
    if extra.count.is_some() {
        base.count = extra.count;
    }
    if extra.posts.is_some() {
        base.posts = extra.posts;
    }
    if extra.username.is_some() {
        base.username = extra.username;
    }
    if extra.password.is_some() {
        base.password = extra.password;
    }
    if extra.grant_type.is_some() {
        base.grant_type = extra.grant_type;
    }
    if extra.code.is_some() {
        base.code = extra.code;
    }
    if extra.item_id.is_some() {
        base.item_id = extra.item_id;
    }
    if extra.object_type.is_some() {
        base.object_type = extra.object_type;
    }
    base
}

fn vk_error(error: AppError) -> (StatusCode, Json<Value>) {
    let (code, msg) = match &error {
        AppError::Unauthorized => (
            5,
            "User authorization failed: no access_token passed.".to_owned(),
        ),
        AppError::Forbidden | AppError::Banned(_) => (15, error.to_string()),
        AppError::NotFound => (3, "Unknown method passed.".to_owned()),
        AppError::Validation(message) => (100, message.clone()),
        _ => (10, "Internal server error".to_owned()),
    };
    (
        StatusCode::OK,
        Json(json!({
            "error": {
                "error_code": code,
                "error_msg": msg,
            }
        })),
    )
}
