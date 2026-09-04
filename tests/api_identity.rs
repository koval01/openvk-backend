mod common;

use common::start_app;
use openvk_backend::{BUILD_HEADER, BUILD_ID, INSTANCE_HEADER, TraceId};

#[tokio::test]
async fn responses_identify_build_and_instance() {
    let (base, _) = start_app().await;
    let client = reqwest::Client::new();

    let first = client.get(format!("{base}/health")).send().await.unwrap();
    let second = client.get(format!("{base}/ready")).send().await.unwrap();

    let build = first
        .headers()
        .get(BUILD_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("x-openvk-build");
    assert_eq!(build, BUILD_ID);
    assert_eq!(
        second
            .headers()
            .get(BUILD_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(BUILD_ID)
    );

    let instance = first
        .headers()
        .get(INSTANCE_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("x-openvk-instance");
    assert!(!instance.is_empty());
    assert!(
        instance
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+'))
    );
    assert_eq!(
        second
            .headers()
            .get(INSTANCE_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(instance)
    );
    assert!(first.headers().get("x-request-id").is_none());
    let trace = first
        .headers()
        .get("x-trace-id")
        .and_then(|value| value.to_str().ok())
        .expect("x-trace-id");
    assert!(TraceId::parse(trace).is_some(), "{trace}");
    let timing = first
        .headers()
        .get("server-timing")
        .and_then(|value| value.to_str().ok())
        .expect("server-timing");
    assert!(timing.starts_with("app;dur="), "{timing}");
}

#[tokio::test]
async fn processes_on_the_same_host_share_instance_id() {
    let (base_a, _) = start_app().await;
    let (base_b, _) = start_app().await;
    let client = reqwest::Client::new();

    let instance_a = client
        .get(format!("{base_a}/health"))
        .send()
        .await
        .unwrap()
        .headers()
        .get(INSTANCE_HEADER)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let instance_b = client
        .get(format!("{base_b}/health"))
        .send()
        .await
        .unwrap()
        .headers()
        .get(INSTANCE_HEADER)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    assert_eq!(instance_a, instance_b);
}
