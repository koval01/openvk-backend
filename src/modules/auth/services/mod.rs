use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, Header, Validation, decode, encode};

use crate::error::AppError;
use crate::modules::auth::models::Claims;
use crate::modules::auth::repository::AuthRepository;
use crate::state::AppState;

pub async fn login(state: &AppState, login: &str, password: &str) -> Result<i64, AppError> {
    if login.trim().is_empty() || password.is_empty() {
        return Err(AppError::Validation(
            "login and password are required".into(),
        ));
    }

    AuthRepository::new(&state.db, &state.vault)
        .authenticate(login, password)
        .await
}

pub async fn register(state: &AppState, login: &str, password: &str) -> Result<i64, AppError> {
    if login.trim().is_empty() || password.len() < 8 {
        return Err(AppError::Validation(
            "login is required and password must be at least 8 characters".into(),
        ));
    }

    AuthRepository::new(&state.db, &state.vault)
        .register(login, password)
        .await
}

pub async fn issue_token(state: &AppState, user_id: i64) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(24)).timestamp(),
    };

    let token = encode(&Header::default(), &claims, &state.jwt.encoding)
        .map_err(|err| AppError::internal(err.to_string()))?;
    state.caches.sessions.insert(token.clone(), user_id).await;
    Ok(token)
}

pub async fn verify_token(state: &AppState, token: &str) -> Result<i64, AppError> {
    if let Some(user_id) = state.caches.sessions.get(token).await {
        return Ok(user_id);
    }

    let data = decode::<Claims>(
        token,
        &state.jwt.decoding,
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| AppError::Unauthorized)?;

    let user_id = data
        .claims
        .sub
        .parse::<i64>()
        .map_err(|_| AppError::Unauthorized)?;

    state
        .caches
        .sessions
        .insert(token.to_owned(), user_id)
        .await;
    Ok(user_id)
}
