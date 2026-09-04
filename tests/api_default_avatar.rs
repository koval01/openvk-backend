mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::get;
use bytes::Bytes;
use common::{
    auth_header, decode_response, register, start_app, start_app_with_config, unique_login,
};
use openvk_backend::pb;
use tokio::net::TcpListener;

const MEDIA_PREFIX: &str = "https://media.openvk.test/";

#[tokio::test]
async fn register_stores_the_dicebear_avatar_as_webp() {
    let (dicebear, hits) = serve_svg().await;
    let template = format!("{dicebear}/svg?backgroundColor=ececed&seed={{seed}}");
    let (base, state) = start_app_with_config(move |config| {
        config.dicebear_url = Some(template);
    })
    .await;

    assert_eq!(hits.load(Ordering::SeqCst), 0, "connect must not fetch");

    let (token, user_id) = register(&base, &unique_login(), "password123").await;
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let profile = load_user(&base, &token, user_id).await;
    let avatar_url = profile.avatar_url.expect("avatar_url");
    assert!(
        avatar_url.starts_with(MEDIA_PREFIX),
        "{avatar_url} should be a stored object"
    );
    assert!(
        !avatar_url.contains("dicebear"),
        "{avatar_url} must not point at DiceBear"
    );

    let key = avatar_url.strip_prefix(MEDIA_PREFIX).expect("storage key");
    assert!(
        std::path::Path::new(key)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("webp")),
        "{key}"
    );
    let stored = state.storage.get(key).await.expect("stored avatar");
    assert_eq!(stored.content_type, "image/webp");
    assert!(stored.bytes.starts_with(b"RIFF"), "webp riff header");
    assert_eq!(&stored.bytes[8..12], b"WEBP");

    let _ = load_user(&base, &token, user_id).await;
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "profile reads must not refetch"
    );
}

#[tokio::test]
async fn backfill_converts_a_stored_svg_avatar_to_webp() {
    let (base, state) = start_app().await;
    let (token, user_id) = register(&base, &unique_login(), "password123").await;
    let svg_key = format!("{user_id}/avatar/legacy.svg");
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"#ececed\"/></svg>";
    state
        .storage
        .put(&svg_key, Bytes::from(svg), "image/svg+xml")
        .await
        .expect("store svg");
    state
        .users()
        .set_avatar_key(user_id, Some(svg_key.clone()))
        .await
        .expect("point at svg");

    openvk_backend::backfill_default_avatars(&state)
        .await
        .expect("backfill");

    let profile = load_user(&base, &token, user_id).await;
    let avatar_url = profile.avatar_url.expect("converted avatar");
    let key = avatar_url.strip_prefix(MEDIA_PREFIX).expect("storage key");
    assert!(
        std::path::Path::new(key)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("webp")),
        "{key}"
    );
    let stored = state.storage.get(key).await.expect("stored webp");
    assert_eq!(stored.content_type, "image/webp");
    assert_eq!(&stored.bytes[8..12], b"WEBP");
    assert!(
        !state.storage.exists(&svg_key).await.expect("svg gone"),
        "legacy svg must be deleted"
    );
}

#[tokio::test]
async fn register_succeeds_when_dicebear_returns_an_error() {
    let dicebear = serve_status(StatusCode::BAD_GATEWAY).await;
    let template = format!("{dicebear}/svg?seed={{seed}}");
    let (base, _) = start_app_with_config(move |config| {
        config.dicebear_url = Some(template);
    })
    .await;

    let (token, user_id) = register(&base, &unique_login(), "password123").await;
    let profile = load_user(&base, &token, user_id).await;
    assert_eq!(profile.id, user_id);
    assert!(profile.avatar_url.is_none());
}

#[tokio::test]
async fn tests_disable_dicebear_so_register_has_no_avatar() {
    let (base, _) = start_app().await;
    let (token, user_id) = register(&base, &unique_login(), "password123").await;
    let profile = load_user(&base, &token, user_id).await;
    assert!(profile.avatar_url.is_none());
}

async fn load_user(base: &str, token: &str, user_id: i64) -> pb::User {
    decode_response(
        reqwest::Client::new()
            .get(format!("{base}/api/v1/users/{user_id}"))
            .header("authorization", auth_header(token))
            .send()
            .await
            .expect("get user"),
    )
    .await
}

async fn serve_svg() -> (String, Arc<AtomicU64>) {
    let hits = Arc::new(AtomicU64::new(0));
    let hits_for_handler = hits.clone();
    let app = Router::new().route(
        "/svg",
        get(move |uri: Uri| {
            let hits = hits_for_handler.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                let seed = query_value(uri.query().unwrap_or_default(), "seed").unwrap_or_default();
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    format!(
                        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\" data-seed=\"{seed}\"><rect width=\"10\" height=\"10\" fill=\"#ececed\"/></svg>"
                    ),
                )
            }
        }),
    );
    (bind_http(app).await, hits)
}

async fn serve_status(status: StatusCode) -> String {
    bind_http(Router::new().route("/svg", get(move || async move { status.into_response() }))).await
}

fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=");
    query.split('&').find_map(|part| part.strip_prefix(&prefix))
}

async fn bind_http(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .expect("serve mock");
    });
    format!("http://{addr}")
}
