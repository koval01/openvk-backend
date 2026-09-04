mod common;

use common::{decode_response, proto_headers, register, start_app, unique_login};
use openvk_backend::{PUBLIC_ID_MAX, PUBLIC_ID_MIN, encode_pb, is_public_id, pb};
use reqwest::StatusCode;

#[tokio::test]
async fn new_accounts_get_random_32bit_ids() {
    let (base, _) = start_app().await;
    let first = unique_login();
    let second = unique_login();
    let (token_a, id_a) = register(&base, &first, "password123").await;
    let (_, id_b) = register(&base, &second, "password123").await;

    assert!(is_public_id(id_a), "{id_a}");
    assert!(is_public_id(id_b), "{id_b}");
    assert_ne!(id_a, id_b);
    assert!((PUBLIC_ID_MIN..=PUBLIC_ID_MAX).contains(&id_a));

    let profile: pb::User = decode_response(
        reqwest::Client::new()
            .get(format!("{base}/api/v1/users/{id_a}"))
            .header("Authorization", format!("Bearer {token_a}"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(profile.id, id_a);
}

#[tokio::test]
async fn wall_posts_are_sequential_on_one_profile() {
    let (base, _) = start_app().await;
    let login = unique_login();
    let (token, user_id) = register(&base, &login, "password123").await;
    let client = reqwest::Client::new();

    let csrf = common::fetch_challenge(&base).await.csrf_token;
    let first = client
        .post(format!("{base}/api/v1/users/{user_id}/wall"))
        .header("Authorization", format!("Bearer {token}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteWall {
            content: "first note on my wall".into(),
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first: pb::WallPost = decode_response(first).await;
    assert_eq!(first.id, 1);
    assert_eq!(first.target_id, user_id);
    assert_eq!(first.permalink, format!("wall{user_id}_1"));

    let second = client
        .post(format!("{base}/api/v1/users/{user_id}/wall"))
        .header("Authorization", format!("Bearer {token}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteWall {
            content: "second note".into(),
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    let second: pb::WallPost = decode_response(second).await;
    assert_eq!(second.id, 2);
    assert_eq!(second.permalink, format!("wall{user_id}_2"));

    let fetched = client
        .get(format!("{base}/api/v1/users/{user_id}/wall/2"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let fetched: pb::WallPost = decode_response(fetched).await;
    assert_eq!(fetched.id, 2);
    assert_eq!(fetched.content, "second note");
}
