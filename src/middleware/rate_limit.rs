use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AppError;
use crate::state::AppState;

pub async fn rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_owned(), |info| info.0.ip().to_string());

    let counter = state
        .caches
        .rate_limit
        .get_with(ip, async { Arc::new(AtomicU32::new(0)) })
        .await;

    let hits = counter.fetch_add(1, Ordering::Relaxed) + 1;
    let limit = state.config.rate_limit_per_minute.get();
    if hits > limit {
        return Err(AppError::RateLimited);
    }

    Ok(next.run(request).await)
}
