mod common;

use common::{decode_error, start_app};
use openvk_backend::{SERVER_TIMING_HEADER, TRACE_HEADER, TraceId};
use reqwest::StatusCode;

#[tokio::test]
async fn health_sends_trace_id_and_server_timing() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").is_none());
    let trace = header(&response, TRACE_HEADER.as_str());
    assert!(TraceId::parse(&trace).is_some(), "{trace}");
    let timing = header(&response, SERVER_TIMING_HEADER.as_str());
    assert!(timing.starts_with("app;dur="), "{timing}");
    let dur = timing
        .strip_prefix("app;dur=")
        .unwrap()
        .parse::<f64>()
        .unwrap();
    assert!(dur >= 0.0, "{timing}");
}

#[tokio::test]
async fn ignores_an_incoming_trace_id() {
    let (base, _) = start_app().await;
    let incoming = "0193a8c0-1234-7abc-8000-000000000001";
    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .header(TRACE_HEADER.as_str(), incoming)
        .send()
        .await
        .unwrap();
    let trace = header(&response, TRACE_HEADER.as_str());
    assert_ne!(trace, incoming);
    assert!(TraceId::parse(&trace).is_some(), "{trace}");
}

#[tokio::test]
async fn replaces_an_invalid_incoming_trace_id() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .header(TRACE_HEADER.as_str(), "not a uuid")
        .send()
        .await
        .unwrap();
    let trace = header(&response, TRACE_HEADER.as_str());
    assert_ne!(trace, "not a uuid");
    assert!(TraceId::parse(&trace).is_some(), "{trace}");
}

#[tokio::test]
async fn error_body_carries_the_same_trace_id_as_the_header() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/feed"))
        .header(reqwest::header::ACCEPT, openvk_backend::PROTOBUF_MIME)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let trace = header(&response, TRACE_HEADER.as_str());
    assert!(TraceId::parse(&trace).is_some(), "{trace}");
    let error = decode_error(response).await;
    assert_eq!(error.error, "unauthorized");
    assert_eq!(error.trace_id, trace);
}

#[tokio::test]
async fn cors_exposes_trace_and_timing_headers() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/health"))
        .header("Origin", "http://127.0.0.1:5173")
        .send()
        .await
        .unwrap();
    let expose = header(&response, "access-control-expose-headers").to_ascii_lowercase();
    assert!(expose.contains("x-trace-id"), "{expose}");
    assert!(expose.contains("server-timing"), "{expose}");
}

fn header(response: &reqwest::Response, name: &str) -> String {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}
