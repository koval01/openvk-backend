mod common;

use common::{assert_protobuf_content_type, decode_response, start_app};
use openvk_backend::{PROTOBUF_MIME, pb};
use reqwest::header::ACCEPT;

#[tokio::test]
async fn about_is_public_and_counts_demo_instance() {
    let (base, _) = start_app().await;
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/about"))
        .header(ACCEPT, PROTOBUF_MIME)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_protobuf_content_type(&response);
    let about: pb::InstanceAbout = decode_response(response).await;
    assert!(about.users >= 3);
    assert!(about.groups >= 1);
    assert!(about.wall_posts >= 3);
    assert!(about.active_users >= 3);
    assert!(
        about
            .popular_groups
            .iter()
            .any(|group| group.name == "OpenVK" && group.members >= 3)
    );
}
