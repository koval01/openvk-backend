//! Request traces: server-issued `x-trace-id` plus a task-local id for error logs.

use std::future::Future;
use std::time::Duration;

use axum::http::HeaderName;
use axum::http::HeaderValue;
use uuid::Uuid;

pub const TRACE_HEADER: HeaderName = HeaderName::from_static("x-trace-id");
pub const SERVER_TIMING_HEADER: HeaderName = HeaderName::from_static("server-timing");

tokio::task_local! {
    static CURRENT_TRACE: TraceId;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceId(Uuid);

impl TraceId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn parse(value: &str) -> Option<Self> {
        Uuid::try_parse(value.trim()).ok().map(Self)
    }

    pub fn header_value(self) -> HeaderValue {
        HeaderValue::from_str(&self.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("00000000-0000-0000-0000-000000000000"))
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub fn current() -> Option<TraceId> {
    CURRENT_TRACE.try_with(|id| *id).ok()
}

pub async fn scope<F, T>(trace_id: TraceId, fut: F) -> T
where
    F: Future<Output = T>,
{
    CURRENT_TRACE.scope(trace_id, fut).await
}

pub fn server_timing(elapsed: Duration) -> HeaderValue {
    let millis = elapsed.as_secs_f64() * 1000.0;
    HeaderValue::from_str(&format!("app;dur={millis:.3}"))
        .unwrap_or_else(|_| HeaderValue::from_static("app;dur=0"))
}

#[cfg(test)]
mod tests {
    use super::{TraceId, server_timing};
    use std::time::Duration;

    #[test]
    fn parses_hyphenated_uuid() {
        let id = TraceId::parse("0193a8c0-1234-7abc-8000-000000000001").expect("uuid");
        assert_eq!(id.to_string(), "0193a8c0-1234-7abc-8000-000000000001");
    }

    #[test]
    fn rejects_garbage() {
        assert!(TraceId::parse("not-a-trace").is_none());
        assert!(TraceId::parse("").is_none());
        assert!(TraceId::parse("'; DROP TABLE").is_none());
    }

    #[test]
    fn server_timing_uses_millis() {
        let value = server_timing(Duration::from_millis(12));
        let text = value.to_str().unwrap();
        assert!(text.starts_with("app;dur="), "{text}");
        assert!(text.contains("12."), "{text}");
    }
}
