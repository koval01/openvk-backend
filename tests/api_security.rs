mod common;

use common::{login, start_app};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderValue};

#[tokio::test]
async fn health_sends_browser_security_headers() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let csp = response
        .headers()
        .get("content-security-policy")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .and_then(|value| value.to_str().ok()),
        Some("DENY")
    );
}

#[tokio::test]
async fn cors_preflight_allows_the_configured_origin() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, format!("{base}/api/v1/feed"))
        .header("Origin", "http://127.0.0.1:5173")
        .header("Access-Control-Request-Method", "GET")
        .header(
            "Access-Control-Request-Headers",
            "authorization,x-csrf-token",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|value| value.to_str().ok()),
        Some("http://127.0.0.1:5173")
    );
}

#[tokio::test]
async fn cors_preflight_allows_localhost_as_loopback_alias() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, format!("{base}/api/v1/feed"))
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "GET")
        .header(
            "Access-Control-Request-Headers",
            "authorization,x-csrf-token",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:5173")
    );
}

#[tokio::test]
async fn mutating_requests_need_a_csrf_token() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/v1/auth/login"))
        .json(&serde_json::json!({ "login": "id1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn foreign_origin_is_rejected_on_mutations() {
    let (base, _) = start_app().await;
    let challenge = common::fetch_challenge(&base).await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-csrf-token",
        HeaderValue::from_str(&challenge.csrf_token).unwrap(),
    );
    headers.insert("origin", HeaderValue::from_static("https://evil.example"));
    let response = reqwest::Client::new()
        .post(format!("{base}/api/v1/auth/login"))
        .headers(headers)
        .json(&serde_json::json!({ "login": "id1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn json_bodies_are_rejected_when_csrf_is_present() {
    let (base, _) = start_app().await;
    let challenge = common::fetch_challenge(&base).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/api/v1/auth/login"))
        .headers(common::csrf_headers(&challenge.csrf_token))
        .header("content-type", "application/json")
        .json(&serde_json::json!({ "login": "id1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn challenge_and_login_set_httponly_cookies() {
    let (base, _) = start_app().await;
    let challenge = reqwest::Client::new()
        .get(format!("{base}/api/v1/security/challenge"))
        .send()
        .await
        .unwrap();
    assert!(
        challenge
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|cookie| cookie.contains("ovk_csrf=") && cookie.contains("HttpOnly")),
        "{:?}",
        challenge.headers().get_all("set-cookie")
    );
    let response = login(&base, "id1", "openvk").await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.contains("ovk_token=") && cookie.contains("HttpOnly")),
        "{cookies:?}"
    );
}
