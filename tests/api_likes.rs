mod common;

use common::{decode_response, proto_headers, register, start_app, unique_login};
use openvk_backend::{encode_pb, pb};
use reqwest::StatusCode;

#[tokio::test]
async fn wall_photo_and_comment_likes_toggle_and_list() {
    let (base, _) = start_app().await;
    let (token_a, user_a) = register(&base, &unique_login(), "password123").await;
    let (token_b, user_b) = register(&base, &unique_login(), "password123").await;
    let client = reqwest::Client::new();
    let csrf = common::fetch_challenge(&base).await.csrf_token;

    let created = client
        .post(format!("{base}/api/v1/users/{user_a}/wall"))
        .header("Authorization", format!("Bearer {token_a}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteWall {
            content: "guestbook hello".into(),
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let post: pb::WallPost = decode_response(created).await;
    let like_url = format!("{base}/api/v1/users/{user_a}/wall/{}/like", post.id);
    let likes_url = format!("{base}/api/v1/users/{user_a}/wall/{}/likes", post.id);

    let liked = client
        .post(&like_url)
        .header("Authorization", format!("Bearer {token_b}"))
        .headers(proto_headers(&csrf))
        .send()
        .await
        .unwrap();
    assert_eq!(liked.status(), StatusCode::OK);
    let liked: pb::LikeState = decode_response(liked).await;
    assert!(liked.liked);
    assert_eq!(liked.count, 1);

    let state_b = client
        .get(&like_url)
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    let state_b: pb::LikeState = decode_response(state_b).await;
    assert!(state_b.liked);
    assert_eq!(state_b.count, 1);

    let listed = client
        .get(format!("{base}/api/v1/users/{user_a}/wall"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    let listed: pb::WallPostList = decode_response(listed).await;
    assert_eq!(listed.posts[0].like_count, 1);
    assert!(listed.posts[0].liked);

    let listed_a = client
        .get(format!("{base}/api/v1/users/{user_a}/wall"))
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    let listed_a: pb::WallPostList = decode_response(listed_a).await;
    assert_eq!(listed_a.posts[0].like_count, 1);
    assert!(!listed_a.posts[0].liked);

    let likers = client
        .get(&likes_url)
        .header("Authorization", format!("Bearer {token_a}"))
        .send()
        .await
        .unwrap();
    let likers: pb::UserList = decode_response(likers).await;
    assert_eq!(likers.users.len(), 1);
    assert_eq!(likers.users[0].id, user_b);

    let unliked = client
        .post(&like_url)
        .header("Authorization", format!("Bearer {token_b}"))
        .headers(proto_headers(&csrf))
        .send()
        .await
        .unwrap();
    let unliked: pb::LikeState = decode_response(unliked).await;
    assert!(!unliked.liked);
    assert_eq!(unliked.count, 0);

    let comment = client
        .post(format!(
            "{base}/api/v1/users/{user_a}/wall/{}/comments",
            post.id
        ))
        .header("Authorization", format!("Bearer {token_b}"))
        .headers(proto_headers(&csrf))
        .body(encode_pb(&pb::WriteComment {
            content: "nice guestbook".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(comment.status(), StatusCode::CREATED);
    let comment: pb::Comment = decode_response(comment).await;

    let comment_like = client
        .post(format!("{base}/api/v1/comments/{}/like", comment.id))
        .header("Authorization", format!("Bearer {token_a}"))
        .headers(proto_headers(&csrf))
        .send()
        .await
        .unwrap();
    let comment_like: pb::LikeState = decode_response(comment_like).await;
    assert!(comment_like.liked);
    assert_eq!(comment_like.count, 1);

    let comments = client
        .get(format!(
            "{base}/api/v1/users/{user_a}/wall/{}/comments",
            post.id
        ))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    let comments: pb::CommentList = decode_response(comments).await;
    assert_eq!(comments.comments[0].like_count, 1);
    assert!(!comments.comments[0].liked);

    let notifications = client
        .get(format!("{base}/api/v1/notifications"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    let notifications: pb::NotificationList = decode_response(notifications).await;
    assert!(
        notifications
            .notifications
            .iter()
            .any(|item| item.kind == "like"),
        "{notifications:?}"
    );
}
