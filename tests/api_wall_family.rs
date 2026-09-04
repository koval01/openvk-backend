mod common;

use common::{
    TINY_PNG, decode_response, proto_headers, register, start_app, unique_login, upload_named,
};
use openvk_backend::{encode_pb, pb};
use reqwest::StatusCode;

#[tokio::test]
async fn wall_keeps_attachments_geo_comments_and_notifications() {
    let (base, _) = start_app().await;
    let (token_a, user_a) = register(&base, &unique_login(), "password123").await;
    let (token_b, _) = register(&base, &unique_login(), "password123").await;
    let client = reqwest::Client::new();
    let csrf = common::fetch_challenge(&base).await.csrf_token;

    let uploaded = upload_named(
        &base,
        &token_a,
        "/api/v1/photos",
        "file",
        "dot.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let photo: pb::Photo = decode_response(uploaded).await;

    let created = client
        .post(format!("{base}/api/v1/users/{user_a}/wall"))
        .header("Authorization", format!("Bearer {token_a}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteWall {
            attachments: vec![pb::WallAttachment {
                kind: "photo".into(),
                owner_id: photo.owner_user_id,
                object_id: photo.id,
                url: photo.url.clone(),
                title: "dot".into(),
                src: String::new(),
            }],
            geo: Some(pb::GeoPoint {
                lat: 55.75,
                lng: 37.62,
                name: "Moscow".into(),
            }),
            source: Some("https://example.test".into()),
            nsfw: true,
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: pb::WallPost = decode_response(created).await;
    assert_eq!(created.attachments.len(), 1);
    assert_eq!(created.attachments[0].kind, "photo");
    assert_eq!(created.attachments[0].object_id, photo.id);
    assert_eq!(
        created.geo.as_ref().map(|geo| geo.name.as_str()),
        Some("Moscow")
    );
    assert_eq!(created.source.as_deref(), Some("https://example.test"));
    assert!(created.nsfw);
    assert_eq!(created.permalink, format!("wall{user_a}_{}", created.id));

    let permalink = client
        .get(format!("{base}/api/v1/users/{user_a}/wall/{}", created.id))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    assert_eq!(permalink.status(), StatusCode::OK);
    let permalink: pb::WallPost = decode_response(permalink).await;
    assert_eq!(permalink.attachments.len(), 1);
    assert_eq!(permalink.comment_count, 0);

    let comment = client
        .post(format!(
            "{base}/api/v1/users/{user_a}/wall/{}/comments",
            created.id
        ))
        .header("Authorization", format!("Bearer {token_b}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteComment {
            content: "hello on the guestbook".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(comment.status(), StatusCode::CREATED);

    let comments = client
        .get(format!(
            "{base}/api/v1/users/{user_a}/wall/{}/comments",
            created.id
        ))
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    let comments: pb::CommentList = decode_response(comments).await;
    assert_eq!(comments.comments.len(), 1);
    assert_eq!(comments.comments[0].content, "hello on the guestbook");

    let photo_page = client
        .get(format!(
            "{base}/api/v1/photos/{}/{}",
            photo.owner_user_id, photo.id
        ))
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    assert_eq!(photo_page.status(), StatusCode::OK);

    let photo_comment = client
        .post(format!(
            "{base}/api/v1/photos/{}/{}/comments",
            photo.owner_user_id, photo.id
        ))
        .header("Authorization", format!("Bearer {token_b}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteComment {
            content: "nice photo".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(photo_comment.status(), StatusCode::CREATED);

    let notifications = client
        .get(format!("{base}/api/v1/notifications"))
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    let notifications: pb::NotificationList = decode_response(notifications).await;
    assert!(
        notifications
            .notifications
            .iter()
            .any(|item| item.kind == "comment" && item.read_at.is_none()),
        "{notifications:?}"
    );

    let seen = client
        .post(format!("{base}/api/v1/notifications"))
        .header("Authorization", format!("Bearer {token_a}"))
        .headers(proto_headers(&csrf))
        .send()
        .await
        .unwrap();
    assert_eq!(seen.status(), StatusCode::NO_CONTENT);

    let archived = client
        .get(format!("{base}/api/v1/notifications"))
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    let archived: pb::NotificationList = decode_response(archived).await;
    assert!(
        archived
            .notifications
            .iter()
            .all(|item| item.read_at.is_some()),
        "{archived:?}"
    );
}

#[tokio::test]
async fn club_wall_uses_negative_owner_and_accepts_guestbook_notes() {
    let (base, _) = start_app().await;
    let login_resp = common::login(&base, "id1", "openvk").await;
    assert_eq!(login_resp.status(), StatusCode::OK);
    let session: pb::Token = decode_response(login_resp).await;
    let client = reqwest::Client::new();
    let csrf = common::fetch_challenge(&base).await.csrf_token;

    let groups = client
        .get(format!("{base}/api/v1/groups"))
        .header("Authorization", format!("Bearer {}", session.token))
        .send()
        .await
        .unwrap();
    let groups: pb::GroupList = decode_response(groups).await;
    let club = groups
        .groups
        .iter()
        .find(|group| group.slug == "openvk")
        .expect("demo club");

    let posted = client
        .post(format!("{base}/api/v1/groups/{}/wall", club.id))
        .header("Authorization", format!("Bearer {}", session.token))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteWall {
            content: "hello club wall".into(),
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::CREATED);
    let posted: pb::WallPost = decode_response(posted).await;
    assert_eq!(posted.target_id, -club.id);
    assert_eq!(posted.permalink, format!("wall-{}_{}", club.id, posted.id));
    assert_eq!(posted.club.as_ref().map(|group| group.id), Some(club.id));

    let listed = client
        .get(format!("{base}/api/v1/groups/{}/wall", club.id))
        .header("Authorization", format!("Bearer {}", session.token))
        .send()
        .await
        .unwrap();
    let listed: pb::WallPostList = decode_response(listed).await;
    assert!(
        listed
            .posts
            .iter()
            .any(|post| post.id == posted.id && post.content == "hello club wall")
    );

    let fetched = client
        .get(format!(
            "{base}/api/v1/groups/{}/wall/{}",
            club.id, posted.id
        ))
        .header("Authorization", format!("Bearer {}", session.token))
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
}
