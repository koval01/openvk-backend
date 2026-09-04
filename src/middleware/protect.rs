use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AppError;
use crate::state::AppState;

const CSRF_HEADER: &str = "x-csrf-token";

pub async fn protect(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if is_safe(request.method()) || is_public_path(request.uri().path()) {
        return Ok(next.run(request).await);
    }
    reject_foreign_origin(request.headers(), &state.config.cors_origins)?;
    let token = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim();
    if token.is_empty() {
        return Err(AppError::Forbidden);
    }
    if let Some(cookie) = cookie_value(request.headers(), "ovk_csrf")
        && cookie != token
    {
        return Err(AppError::Forbidden);
    }
    state.security.verify_csrf(token)?;
    Ok(next.run(request).await)
}

fn is_safe(method: &Method) -> bool {
    matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/health"
            | "/ready"
            | "/api/v1/health"
            | "/api/v1/security/challenge"
            | "/token"
            | "/token/"
            | "/oauth/token"
    ) || path.starts_with("/method/")
        || path.starts_with("/api/v1/away/")
}

fn reject_foreign_origin(headers: &HeaderMap, allowed: &[String]) -> Result<(), AppError> {
    if allowed.iter().any(|origin| origin == "*") {
        return Ok(());
    }
    let Some(origin) = origin_of(headers) else {
        return Ok(());
    };
    if allowed.iter().any(|allowed| allowed == &origin) {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

fn origin_of(headers: &HeaderMap) -> Option<String> {
    if let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        let origin = origin.trim();
        if !origin.is_empty() && origin != "null" {
            return Some(origin.to_owned());
        }
    }
    let referer = headers
        .get(axum::http::header::REFERER)
        .and_then(|value| value.to_str().ok())?;
    let url = referer.parse::<axum::http::Uri>().ok()?;
    let scheme = url.scheme_str()?;
    let host = url.authority().map(axum::http::uri::Authority::as_str)?;
    Some(format!("{scheme}://{host}"))
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?;
    let prefix = format!("{name}=");
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&prefix) {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}
