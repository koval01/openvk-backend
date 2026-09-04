pub mod about;
pub mod auth;
pub mod feed;
pub mod friends;
pub mod groups;
pub mod media;
pub mod messenger;
pub mod notifications;
pub mod users;
pub mod wall;

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::health;
use crate::state::AppState;

pub fn http_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/media/{*key}", get(media::handlers::serve_object))
        .nest("/api/v1", api_v1())
}

pub fn ws_router() -> Router<AppState> {
    Router::new().route("/ws", get(messenger::handlers::upgrade))
}

fn api_v1() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::api_health))
        .route("/about", get(about::handlers::instance))
        .route("/auth/login", post(auth::handlers::login))
        .route("/auth/register", post(auth::handlers::register))
        .route("/auth/logout", post(auth::handlers::logout))
        .route("/security/challenge", get(auth::handlers::challenge))
        .route("/feed", get(feed::handlers::news))
        .route("/users/{id}", get(users::handlers::get_user))
        .route("/users/{id}/friends", get(friends::handlers::list_for_user))
        .route(
            "/users/{id}/wall",
            get(wall::handlers::list_wall).post(wall::handlers::write_wall),
        )
        .route(
            "/users/{id}/wall/{local_id}",
            get(wall::handlers::get_wall_post),
        )
        .route("/friends", get(friends::handlers::list))
        .route(
            "/messages",
            get(messenger::handlers::list).post(messenger::handlers::send),
        )
        .route(
            "/settings",
            get(users::handlers::get_settings)
                .put(users::handlers::update_settings)
                .delete(users::handlers::delete_account),
        )
        .route("/settings/password", put(users::handlers::change_password))
        .route("/settings/avatar", post(media::handlers::upload_avatar))
        .route(
            "/audio",
            get(media::handlers::list_audio).post(media::handlers::upload_audio),
        )
        .route("/audio/{id}", delete(media::handlers::delete_audio))
        .route(
            "/albums",
            get(media::handlers::list_albums).post(media::handlers::create_album),
        )
        .route(
            "/albums/{id}",
            get(media::handlers::get_album).delete(media::handlers::delete_album),
        )
        .route(
            "/albums/{id}/photos",
            post(media::handlers::upload_album_photo),
        )
        .route(
            "/albums/{id}/photos/{media_id}",
            delete(media::handlers::delete_album_photo),
        )
        .route("/photos", post(media::handlers::upload_photo))
        .route(
            "/videos",
            get(media::handlers::list_videos).post(media::handlers::upload_video),
        )
        .route("/videos/{id}", delete(media::handlers::delete_video))
        .route("/groups", get(groups::handlers::list))
        .route("/notifications", get(notifications::handlers::list))
}
