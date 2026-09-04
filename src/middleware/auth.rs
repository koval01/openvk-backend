use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;

use crate::error::AppError;
use crate::modules::auth::services;
use crate::modules::users;
use crate::state::AppState;

#[derive(Clone, Copy, Debug)]
pub struct AuthUser {
    pub user_id: i64,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_or_cookie(parts).ok_or(AppError::Unauthorized)?;
        let user_id = services::verify_token(state, &token).await?;
        match users::services::get_profile(state, user_id).await {
            Ok(_) => Ok(Self { user_id }),
            Err(AppError::NotFound) => Err(AppError::Unauthorized),
            Err(error) => Err(error),
        }
    }
}

pub(crate) fn token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(header) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        && let Some(token) = header.strip_prefix("Bearer ")
    {
        let token = token.trim();
        if !token.is_empty() {
            return Some(token.to_owned());
        }
    }

    let cookie = headers.get(COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("ovk_token=") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn bearer_or_cookie(parts: &Parts) -> Option<String> {
    token_from_headers(&parts.headers)
}

#[cfg(test)]
mod tests {
    use super::bearer_or_cookie;
    use axum::http::Request;

    #[test]
    fn reads_bearer_token() {
        let request = Request::builder()
            .header("authorization", "Bearer abc.def")
            .body(())
            .unwrap();
        let (parts, ()) = request.into_parts();
        assert_eq!(bearer_or_cookie(&parts).as_deref(), Some("abc.def"));
    }

    #[test]
    fn reads_cookie_token() {
        let request = Request::builder()
            .header("cookie", "theme=dark; ovk_token=jwt.value; other=1")
            .body(())
            .unwrap();
        let (parts, ()) = request.into_parts();
        assert_eq!(bearer_or_cookie(&parts).as_deref(), Some("jwt.value"));
    }
}
