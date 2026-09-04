mod common;

use common::{
    TINY_PNG, TINY_WAV, TINY_WEBM, decode_response, proto_headers, register, start_app,
    unique_login, upload_named,
};
use openvk_backend::{encode_pb, pb};
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;

#[tokio::test]
async fn media_upload_display_and_delete_clears_storage() {
    let (base, state) = start_app().await;
    assert_eq!(state.storage.backend_name(), "disk");
    let login = unique_login();
    let password = "password123";
    let (token, _) = register(&base, &login, password).await;

    let photo = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "tiny.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    assert_eq!(
        photo.status(),
        StatusCode::CREATED,
        "{}",
        photo.text().await.unwrap()
    );
    let photo_body: pb::Photo = decode_response(photo).await;
    let photo_id = photo_body.id;
    let photo_url = photo_body.url.clone();
    assert!(photo_url.starts_with("/media/"));
    let photo_key = state.media_storage_key(photo_id).await.unwrap();
    assert!(state.storage.exists(&photo_key).await.unwrap());
    let stored = state.storage.get(&photo_key).await.unwrap();
    assert!(stored.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(!stored.bytes.windows(7).any(|window| window == b"<script"));
    assert_eq!(photo_body.width, Some(1));
    assert_eq!(photo_body.height, Some(1));

    let served = reqwest::Client::new()
        .get(format!("{base}{photo_url}"))
        .send()
        .await
        .unwrap();
    assert_eq!(served.status(), StatusCode::OK, "{}", served.status());
    assert_eq!(
        served
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let served_body = served.bytes().await.unwrap();
    assert!(served_body.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(
        reqwest::Client::new()
            .get(format!("{base}/media/missing/photo/nope.png"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let albums: pb::AlbumList = decode_response(
        reqwest::Client::new()
            .get(format!("{base}/api/v1/albums?photos=true"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(albums.albums[0].photo_count, 1);
    let album_id = albums.albums[0].id;

    let audio = upload_named(
        &base,
        &token,
        "/api/v1/audio",
        "file",
        "tiny.wav",
        "audio/wav",
        TINY_WAV.to_vec(),
        &[("artist", "Test Artist"), ("title", "Test Track")],
    )
    .await;
    assert_eq!(
        audio.status(),
        StatusCode::CREATED,
        "{}",
        audio.text().await.unwrap()
    );
    let audio_body: pb::AudioTrack = decode_response(audio).await;
    let audio_id = audio_body.id;
    let audio_media_id = audio_body.media_id;
    let audio_src = audio_body.src.clone();
    assert!(audio_src.starts_with("/media/"));
    let audio_key = state.media_storage_key(audio_media_id).await.unwrap();

    let video = upload_named(
        &base,
        &token,
        "/api/v1/videos",
        "file",
        "tiny.webm",
        "video/webm",
        TINY_WEBM.to_vec(),
        &[("title", "Clip")],
    )
    .await;
    assert_eq!(
        video.status(),
        StatusCode::CREATED,
        "{}",
        video.text().await.unwrap()
    );
    let video_body: pb::Video = decode_response(video).await;
    let video_id = video_body.id;

    let avatar = upload_named(
        &base,
        &token,
        "/api/v1/settings/avatar",
        "file",
        "avatar.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    assert_eq!(
        avatar.status(),
        StatusCode::OK,
        "{}",
        avatar.text().await.unwrap()
    );
    let avatar_body: pb::User = decode_response(avatar).await;
    let avatar_url = avatar_body.avatar_url.as_deref().unwrap();
    assert!(avatar_url.starts_with("/media/"));
    let avatar_served = reqwest::Client::new()
        .get(format!("{base}{avatar_url}"))
        .send()
        .await
        .unwrap();
    assert_eq!(avatar_served.status(), StatusCode::OK);
    assert!(
        avatar_served
            .bytes()
            .await
            .unwrap()
            .starts_with(&[0x89, b'P', b'N', b'G'])
    );

    let csrf = common::fetch_challenge(&base).await.csrf_token;
    assert_eq!(
        reqwest::Client::new()
            .delete(format!("{base}/api/v1/albums/{album_id}/photos/{photo_id}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(!state.storage.exists(&photo_key).await.unwrap());
    assert_eq!(
        reqwest::Client::new()
            .get(format!("{base}{photo_url}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        reqwest::Client::new()
            .delete(format!("{base}/api/v1/audio/{audio_id}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(!state.storage.exists(&audio_key).await.unwrap());
    assert_eq!(
        reqwest::Client::new()
            .get(format!("{base}{audio_src}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        reqwest::Client::new()
            .delete(format!("{base}/api/v1/videos/{video_id}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("x-csrf-token", &csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn deleting_account_removes_stored_objects() {
    let (base, state) = start_app().await;
    let login = unique_login();
    let password = "password123";
    let (token, _) = register(&base, &login, password).await;

    let photo = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "tiny.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    let photo_body: pb::Photo = decode_response(photo).await;
    let photo_url = photo_body.url.clone();
    let media_id = photo_body.id;
    let key = state.media_storage_key(media_id).await.unwrap();
    assert!(state.storage.exists(&key).await.unwrap());

    let challenge = common::fetch_challenge(&base).await;
    let sealed =
        openvk_backend::seal_fields(&challenge.public_key, &challenge.nonce, &[password]).unwrap();
    let deleted = reqwest::Client::new()
        .delete(format!("{base}/api/v1/settings"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .headers(proto_headers(&challenge.csrf_token))
        .body(encode_pb(&pb::SealedPassword {
            challenge_id: challenge.challenge_id,
            password_sealed: sealed,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        deleted.status(),
        StatusCode::NO_CONTENT,
        "{}",
        deleted.text().await.unwrap()
    );

    let login_again = common::login(&base, &login, password).await;
    assert_eq!(login_again.status(), StatusCode::UNAUTHORIZED);
    assert!(!state.storage.exists(&key).await.unwrap());

    let content = reqwest::Client::new()
        .get(format!("{base}{photo_url}"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert!(
        matches!(
            content.status(),
            StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND
        ),
        "{}",
        content.status()
    );
}

#[tokio::test]
async fn media_lists_default_to_the_signed_in_user() {
    let (base, _state) = start_app().await;
    let (token_a, user_a) = register(&base, &unique_login(), "password123").await;
    let (token_b, _) = register(&base, &unique_login(), "password123").await;

    let photo = upload_named(
        &base,
        &token_a,
        "/api/v1/photos",
        "file",
        "tiny.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    assert_eq!(photo.status(), StatusCode::CREATED);
    let photo_body: pb::Photo = decode_response(photo).await;
    let album_id = photo_body.album_id;

    let own: pb::AlbumList = decode_response(
        reqwest::Client::new()
            .get(format!("{base}/api/v1/albums?photos=true"))
            .header(AUTHORIZATION, format!("Bearer {token_b}"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(own.albums.len(), 0);

    let theirs: pb::AlbumList = decode_response(
        reqwest::Client::new()
            .get(format!(
                "{base}/api/v1/albums?owner_id={user_a}&photos=true"
            ))
            .header(AUTHORIZATION, format!("Bearer {token_b}"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(theirs.albums[0].photo_count, 1);

    let stolen = upload_named(
        &base,
        &token_b,
        &format!("/api/v1/photos?album_id={album_id}"),
        "file",
        "tiny.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    assert_eq!(stolen.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rejected_files_never_create_objects() {
    let (base, _) = start_app().await;
    let (token, _) = register(&base, &unique_login(), "password123").await;
    let response = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "note.txt",
        "text/plain",
        b"hello".to_vec(),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let albums: pb::AlbumList = decode_response(
        reqwest::Client::new()
            .get(format!("{base}/api/v1/albums?photos=true"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(albums.albums.len(), 0);
}

#[tokio::test]
async fn polyglot_and_mismatched_media_are_rejected_or_rewritten() {
    let (base, state) = start_app().await;
    let (token, _) = register(&base, &unique_login(), "password123").await;

    let mut polyglot = TINY_PNG.to_vec();
    polyglot.extend_from_slice(b"<html><script>alert(1)</script>");
    let rewritten = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "shot.png",
        "image/png",
        polyglot,
        &[],
    )
    .await;
    assert_eq!(rewritten.status(), StatusCode::CREATED);
    let body: pb::Photo = decode_response(rewritten).await;
    let key = state.media_storage_key(body.id).await.unwrap();
    let stored = state.storage.get(&key).await.unwrap();
    assert!(!stored.bytes.windows(7).any(|window| window == b"<script"));
    assert!(!stored.bytes.windows(5).any(|window| window == b"<html"));

    let html = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "shot.png",
        "image/png",
        b"<html><script>alert(1)</script>".to_vec(),
        &[],
    )
    .await;
    assert_eq!(html.status(), StatusCode::BAD_REQUEST);

    let disguised = upload_named(
        &base,
        &token,
        "/api/v1/videos",
        "file",
        "clip.webm",
        "video/webm",
        TINY_PNG.to_vec(),
        &[("title", "Clip")],
    )
    .await;
    assert_eq!(disguised.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn deleting_an_album_removes_its_files() {
    let (base, state) = start_app().await;
    let (token, _) = register(&base, &unique_login(), "password123").await;
    let photo = upload_named(
        &base,
        &token,
        "/api/v1/photos",
        "file",
        "tiny.png",
        "image/png",
        TINY_PNG.to_vec(),
        &[],
    )
    .await;
    let photo_body: pb::Photo = decode_response(photo).await;
    let media_id = photo_body.id;
    let album_id = photo_body.album_id;
    let key = state.media_storage_key(media_id).await.unwrap();
    assert!(state.storage.exists(&key).await.unwrap());

    let csrf = common::fetch_challenge(&base).await.csrf_token;
    let deleted = reqwest::Client::new()
        .delete(format!("{base}/api/v1/albums/{album_id}"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("x-csrf-token", csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(!state.storage.exists(&key).await.unwrap());
}
