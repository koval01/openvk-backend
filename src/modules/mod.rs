pub mod about;
pub mod auth;
pub mod comments;
pub mod desk;
pub mod feed;
pub mod friends;
pub mod groups;
pub mod likes;
pub mod media;
pub mod messenger;
pub mod notifications;
pub mod users;
pub mod vkapi;
pub mod wall;

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::health;
use crate::state::AppState;

pub fn http_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .merge(vkapi::router())
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
        .route(
            "/users/{id}/wall/{local_id}/comments",
            get(comments::handlers::list_wall_user).post(comments::handlers::write_wall_user),
        )
        .route(
            "/users/{id}/wall/{local_id}/like",
            get(likes::handlers::get_wall_user).post(likes::handlers::wall_user),
        )
        .route(
            "/users/{id}/wall/{local_id}/likes",
            get(likes::handlers::wall_likers_user),
        )
        .route(
            "/groups/{id}/wall",
            get(wall::handlers::list_group_wall).post(wall::handlers::write_group_wall),
        )
        .route(
            "/groups/{id}/wall/{local_id}",
            get(wall::handlers::get_group_wall_post),
        )
        .route(
            "/groups/{id}/wall/{local_id}/comments",
            get(comments::handlers::list_wall_group).post(comments::handlers::write_wall_group),
        )
        .route(
            "/groups/{id}/wall/{local_id}/like",
            get(likes::handlers::get_wall_group).post(likes::handlers::wall_group),
        )
        .route(
            "/groups/{id}/wall/{local_id}/likes",
            get(likes::handlers::wall_likers_group),
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
        .route("/photos/{owner}/{id}", get(media::handlers::get_photo))
        .route(
            "/photos/{owner}/{id}/comments",
            get(comments::handlers::list_photo).post(comments::handlers::write_photo),
        )
        .route(
            "/photos/{owner}/{id}/like",
            get(likes::handlers::get_photo).post(likes::handlers::photo),
        )
        .route(
            "/photos/{owner}/{id}/likes",
            get(likes::handlers::photo_likers),
        )
        .route(
            "/videos",
            get(media::handlers::list_videos).post(media::handlers::upload_video),
        )
        .route("/videos/{id}", delete(media::handlers::delete_video))
        .route("/videos/{owner}/{id}", get(media::handlers::get_video))
        .route(
            "/videos/{owner}/{id}/comments",
            get(comments::handlers::list_video).post(comments::handlers::write_video),
        )
        .route(
            "/videos/{owner}/{id}/like",
            get(likes::handlers::get_video).post(likes::handlers::video),
        )
        .route(
            "/videos/{owner}/{id}/likes",
            get(likes::handlers::video_likers),
        )
        .route(
            "/comments/{id}/like",
            get(likes::handlers::get_comment).post(likes::handlers::comment),
        )
        .route("/groups", get(groups::handlers::list))
        .route("/groups/{id}", get(groups::handlers::get))
        .route(
            "/notifications",
            get(notifications::handlers::list).post(notifications::handlers::mark_seen),
        )
        .route(
            "/gifts",
            get(desk::handlers::gift_catalog).post(desk::handlers::send_gift),
        )
        .route("/users/{id}/gifts", get(desk::handlers::user_gifts))
        .route("/coins/transfer", post(desk::handlers::transfer_coins))
        .route("/vouchers/redeem", post(desk::handlers::redeem_voucher))
        .route(
            "/tickets",
            get(desk::handlers::list_tickets).post(desk::handlers::create_ticket),
        )
        .route(
            "/tickets/{id}",
            get(desk::handlers::get_ticket).delete(desk::handlers::delete_ticket),
        )
        .route("/tickets/{id}/replies", post(desk::handlers::reply_ticket))
        .route("/tickets/{id}/close", post(desk::handlers::close_ticket))
        .route(
            "/reports",
            get(desk::handlers::list_reports).post(desk::handlers::create_report),
        )
        .route("/reports/{id}", get(desk::handlers::get_report))
        .route("/reports/{id}/action", post(desk::handlers::report_action))
        .route("/unban", post(desk::handlers::self_unban))
        .route("/away/links/{id}", get(desk::handlers::get_banned_link))
        .route("/away/check", get(desk::handlers::check_url))
        .route("/nospam", post(desk::handlers::nospam))
        .route(
            "/nospam/{id}/rollback",
            post(desk::handlers::nospam_rollback),
        )
        .route("/admin/overview", get(desk::handlers::overview))
        .route("/admin/users", get(desk::handlers::admin_users))
        .route("/admin/clubs", get(desk::handlers::admin_clubs))
        .route(
            "/admin/vouchers",
            get(desk::handlers::list_vouchers).post(desk::handlers::create_voucher),
        )
        .route("/admin/vouchers/{id}", get(desk::handlers::get_voucher))
        .route(
            "/admin/banned-links",
            get(desk::handlers::list_banned_links).post(desk::handlers::add_banned_link),
        )
        .route(
            "/admin/banned-links/{id}",
            delete(desk::handlers::delete_banned_link),
        )
        .route("/admin/users/{id}/ban", post(desk::handlers::ban_user))
        .route("/admin/users/{id}/unban", post(desk::handlers::unban_user))
        .route("/admin/users/{id}/warn", post(desk::handlers::warn_user))
        .route(
            "/admin/users/{id}/warnings",
            get(desk::handlers::list_warnings),
        )
        .route("/admin/users/{id}/limits", post(desk::handlers::set_limits))
        .route(
            "/admin/users/{id}/support-ban",
            post(desk::handlers::support_ban),
        )
        .route(
            "/admin/users/{id}/support-unban",
            post(desk::handlers::support_unban),
        )
}
