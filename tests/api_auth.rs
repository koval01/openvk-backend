mod common;

use common::{
    decode_error, decode_response, encode_auth, login, proto_headers, register, start_app,
    start_app_with_turnstile_secret, turnstile_dummy_token, unique_login,
};
use openvk_backend::DUMMY_FAIL_SECRET;
use openvk_backend::pb;
use reqwest::StatusCode;

#[tokio::test]
async fn login_and_register_require_a_turnstile_token() {
    let (base, _) = start_app().await;
    let client = reqwest::Client::new();
    let challenge = common::fetch_challenge(&base).await;
    let sealed = openvk_backend::seal_fields(&challenge.public_key, &challenge.nonce, &["openvk"])
        .expect("seal");

    for path in ["/api/v1/auth/login", "/api/v1/auth/register"] {
        let login_name = if path.ends_with("register") {
            unique_login()
        } else {
            "id1".into()
        };
        let missing = client
            .post(format!("{base}{path}"))
            .headers(proto_headers(&challenge.csrf_token))
            .body(encode_auth(
                &login_name,
                &challenge.challenge_id,
                &sealed,
                "",
            ))
            .send()
            .await
            .unwrap();
        let status = missing.status();
        let body = decode_error(missing).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} {}", body.message);
        assert!(
            body.message.contains("security check"),
            "{path} {}",
            body.message
        );
    }

    let ok = login(&base, "id1", "openvk").await;
    let status = ok.status();
    let body: pb::Token = decode_response(ok).await;
    assert_eq!(status, StatusCode::OK, "{}", body.token);
    assert!(!body.token.is_empty());
}

#[tokio::test]
async fn dummy_fail_secret_rejects_login() {
    let (base, _) = start_app_with_turnstile_secret(DUMMY_FAIL_SECRET).await;
    let response = login(&base, "id1", "openvk").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = decode_error(response).await;
    assert!(body.message.contains("human"), "{}", body.message);
}

#[tokio::test]
async fn register_accepts_the_dummy_always_pass_token() {
    let (base, _) = start_app().await;
    let login_name = unique_login();
    let (token, user_id) = register(&base, &login_name, "password123").await;
    assert!(user_id > 0);
    assert!(!token.is_empty());
}

#[tokio::test]
async fn login_rejects_plaintext_password_without_csrf() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({
            "login": "id1",
            "password": "openvk",
            "turnstile_token": turnstile_dummy_token()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
