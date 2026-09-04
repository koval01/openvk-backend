#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use openvk_backend::{
    AppState, Config, PROTOBUF_MIME, StorageBackend, build_router, decode_pb, encode_pb, pb,
    seal_fields,
};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tokio::net::TcpListener;

static LOGIN_SEQ: AtomicU64 = AtomicU64::new(0);

pub const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

pub const TINY_WAV: &[u8] =
    b"RIFF$\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0D\xac\0\0\x88X\x01\0\x02\0\x10\0data\0\0\0\0";

pub const TINY_WEBM: &[u8] = include_bytes!("../fixtures/tiny.webm");

pub fn turnstile_dummy_token() -> &'static str {
    "XXXX.DUMMY.TOKEN.XXXX"
}

pub async fn start_app() -> (String, AppState) {
    start_app_with_config(|_| {}).await
}

pub async fn start_app_with_turnstile_secret(secret: &str) -> (String, AppState) {
    let secret = secret.to_owned();
    start_app_with_config(move |config| {
        config.turnstile_secret_key = secret;
    })
    .await
}

async fn start_app_with_config(configure: impl FnOnce(&mut Config)) -> (String, AppState) {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("config");
    config.storage_backend = StorageBackend::Disk;
    config.media_root = std::env::temp_dir().join(format!(
        "openvk-media-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    configure(&mut config);
    let state = AppState::connect(config).await.expect("app state");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let app: Router = build_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("serve");
    });
    (format!("http://{addr}"), state)
}

pub fn unique_login() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis();
    format!("t{millis}{}", LOGIN_SEQ.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug)]
pub struct Challenge {
    pub csrf_token: String,
    pub challenge_id: String,
    pub nonce: String,
    pub public_key: String,
}

pub async fn fetch_challenge(base: &str) -> Challenge {
    let response = client()
        .get(format!("{base}/api/v1/security/challenge"))
        .header(reqwest::header::ACCEPT, PROTOBUF_MIME)
        .send()
        .await
        .expect("challenge");
    assert_eq!(
        response.status(),
        200,
        "{}",
        response.text().await.unwrap_or_default()
    );
    let body: pb::Challenge = decode_response(response).await;
    Challenge {
        csrf_token: body.csrf_token,
        challenge_id: body.challenge_id,
        nonce: body.nonce,
        public_key: body.public_key,
    }
}

pub fn csrf_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-csrf-token",
        HeaderValue::from_str(token).expect("csrf header"),
    );
    headers
}

pub fn proto_headers(token: &str) -> HeaderMap {
    let mut headers = csrf_headers(token);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(PROTOBUF_MIME));
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(PROTOBUF_MIME),
    );
    headers
}

pub async fn decode_response<T: prost::Message + Default>(response: reqwest::Response) -> T {
    let bytes = response.bytes().await.expect("protobuf bytes");
    decode_pb(&bytes).expect("protobuf")
}

pub async fn decode_error(response: reqwest::Response) -> pb::Error {
    decode_response(response).await
}

pub fn encode_auth(
    login: &str,
    challenge_id: &str,
    password_sealed: &str,
    turnstile: &str,
) -> Vec<u8> {
    encode_pb(&pb::AuthRequest {
        login: login.to_owned(),
        challenge_id: challenge_id.to_owned(),
        password_sealed: password_sealed.to_owned(),
        turnstile_token: turnstile.to_owned(),
    })
}

pub async fn sealed_auth_body(base: &str, login: &str, password: &str) -> (HeaderMap, Vec<u8>) {
    let challenge = fetch_challenge(base).await;
    let sealed = seal_fields(&challenge.public_key, &challenge.nonce, &[password]).expect("seal");
    (
        proto_headers(&challenge.csrf_token),
        encode_auth(
            login,
            &challenge.challenge_id,
            &sealed,
            turnstile_dummy_token(),
        ),
    )
}

pub async fn register(base: &str, login: &str, password: &str) -> (String, i64) {
    let (headers, body) = sealed_auth_body(base, login, password).await;
    let response = client()
        .post(format!("{base}/api/v1/auth/register"))
        .headers(headers)
        .body(body)
        .send()
        .await
        .expect("register");
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    let body: pb::Token = decode_response(response).await;
    (body.token, body.user_id)
}

pub async fn login(base: &str, login: &str, password: &str) -> reqwest::Response {
    let (headers, body) = sealed_auth_body(base, login, password).await;
    client()
        .post(format!("{base}/api/v1/auth/login"))
        .headers(headers)
        .body(body)
        .send()
        .await
        .expect("login")
}

pub fn client() -> reqwest::Client {
    reqwest::Client::new()
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

#[allow(clippy::too_many_arguments)]
pub async fn upload_named(
    base: &str,
    token: &str,
    path: &str,
    field: &str,
    filename: &str,
    mime: &str,
    bytes: Vec<u8>,
    extras: &[(&str, &str)],
) -> reqwest::Response {
    let field_name = field.to_owned();
    let mut form = reqwest::multipart::Form::new().part(
        field_name,
        reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_owned())
            .mime_str(mime)
            .expect("mime"),
    );
    for (name, value) in extras {
        form = form.text((*name).to_owned(), (*value).to_owned());
    }
    let challenge = fetch_challenge(base).await;
    client()
        .post(format!("{base}{path}"))
        .header(AUTHORIZATION, auth_header(token))
        .header("x-csrf-token", challenge.csrf_token)
        .header(reqwest::header::ACCEPT, PROTOBUF_MIME)
        .multipart(form)
        .send()
        .await
        .expect("upload")
}

#[allow(dead_code)]
pub fn assert_protobuf_content_type(response: &reqwest::Response) {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.contains(PROTOBUF_MIME) || content_type.contains("application/protobuf"),
        "{content_type}"
    );
}

#[allow(dead_code)]
pub fn state_arc(state: AppState) -> Arc<AppState> {
    Arc::new(state)
}
