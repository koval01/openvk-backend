use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::header::{HeaderMap, SET_COOKIE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse};

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::token_from_headers;
use crate::modules::auth::models::TokenResponse;
use crate::modules::auth::services;
use crate::pb;
use crate::security::{clear_csrf_cookie, clear_session_cookie, csrf_cookie, session_cookie};
use crate::state::AppState;

pub async fn challenge(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let issued = state.security.issue_challenge().await;
    let secure = state.config.cookie_secure;
    Ok((
        AppendHeaders([(
            SET_COOKIE,
            cookie_header(csrf_cookie(&issued.csrf_token, secure)),
        )]),
        Proto(codec::challenge_to_pb(&issued)),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Proto(body): Proto<pb::AuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    let body = codec::credentials_from_pb(body);
    state
        .turnstile
        .verify(&body.turnstile_token, Some(&addr.ip().to_string()))
        .await?;
    let password = open_password(&state, &body).await?;
    let user_id = services::login(&state, &body.login, &password).await?;
    issued_session(&state, &headers, user_id).await
}

pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Proto(body): Proto<pb::AuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    let body = codec::credentials_from_pb(body);
    state
        .turnstile
        .verify(&body.turnstile_token, Some(&addr.ip().to_string()))
        .await?;
    let password = open_password(&state, &body).await?;
    let user_id = services::register(&state, &body.login, &password).await?;
    issued_session(&state, &headers, user_id).await
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = token_from_headers(&headers) {
        state.caches.sessions.invalidate(&token).await;
    }
    let secure = cookies_secure(&state, &headers);
    (
        StatusCode::NO_CONTENT,
        AppendHeaders([
            (SET_COOKIE, cookie_header(clear_session_cookie(secure))),
            (SET_COOKIE, cookie_header(clear_csrf_cookie(secure))),
        ]),
    )
}

async fn open_password(
    state: &AppState,
    body: &crate::modules::auth::models::Credentials,
) -> Result<String, AppError> {
    if body.challenge_id.trim().is_empty() || body.password_sealed.trim().is_empty() {
        return Err(AppError::Validation(
            "password must be sealed with the login challenge".into(),
        ));
    }
    let mut fields = state
        .security
        .open_sealed(&body.challenge_id, &body.password_sealed, 1)
        .await?;
    fields.pop().ok_or_else(|| {
        AppError::Validation("password must be sealed with the login challenge".into())
    })
}

async fn issued_session(
    state: &AppState,
    headers: &HeaderMap,
    user_id: i64,
) -> Result<impl IntoResponse + use<>, AppError> {
    let token = services::issue_token(state, user_id).await?;
    let secure = cookies_secure(state, headers);
    Ok((
        AppendHeaders([(
            SET_COOKIE,
            cookie_header(session_cookie(&token, secure, 86_400)),
        )]),
        Proto(codec::token_to_pb(&TokenResponse {
            token,
            token_type: "Bearer",
            user_id,
        })),
    ))
}

fn cookies_secure(state: &AppState, headers: &HeaderMap) -> bool {
    state.config.cookie_secure
        || headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            == Some("https")
}

fn cookie_header(value: String) -> HeaderValue {
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("ovk_token="))
}
