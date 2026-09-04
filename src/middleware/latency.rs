use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::identity::{BUILD_HEADER, INSTANCE_HEADER};
use crate::state::AppState;

pub async fn latency(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let request_id = Uuid::now_v7().to_string();

    let mut response = next.run(request).await;
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        headers.insert("x-request-id", value);
    }
    headers.insert(BUILD_HEADER, state.identity.build.clone());
    headers.insert(INSTANCE_HEADER, state.identity.instance.clone());
    crate::http_security::insert_security_headers(headers);
    tracing::info!(
        %method,
        %path,
        elapsed_ms,
        %request_id,
        instance = state.identity.instance_id(),
        "http"
    );
    response
}
