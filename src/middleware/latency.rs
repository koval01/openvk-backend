use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

use crate::identity::{BUILD_HEADER, INSTANCE_HEADER};
use crate::state::AppState;
use crate::trace::{self, SERVER_TIMING_HEADER, TRACE_HEADER, TraceId};

pub async fn latency(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let trace_id = TraceId::from_request(&request);
    request.extensions_mut().insert(trace_id);

    let span = tracing::info_span!(
        "http.request",
        %trace_id,
        %method,
        %path,
    );
    let mut response = trace::scope(trace_id, next.run(request))
        .instrument(span)
        .await;
    let elapsed = started.elapsed();
    let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let headers = response.headers_mut();
    headers.insert(TRACE_HEADER, trace_id.header_value());
    headers.insert(SERVER_TIMING_HEADER, trace::server_timing(elapsed));
    headers.insert(BUILD_HEADER, state.identity.build.clone());
    headers.insert(INSTANCE_HEADER, state.identity.instance.clone());
    crate::http_security::insert_security_headers(headers);
    tracing::info!(
        %method,
        %path,
        elapsed_ms,
        %trace_id,
        instance = state.identity.instance_id(),
        status = response.status().as_u16(),
        "http"
    );
    response
}
