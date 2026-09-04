mod common;

use common::{
    auth_header, client, decode_response, fetch_challenge, login, proto_headers, register,
    start_app, unique_login,
};
use openvk_backend::AppState;
use openvk_backend::{encode_pb, pb};
use reqwest::StatusCode;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

#[tokio::test]
async fn passwords_are_stored_as_argon2id() {
    let (base, state) = start_app().await;
    let login_name = unique_login();
    let (_, user_id) = register(&base, &login_name, "password123").await;

    let algo = scalar(
        &state,
        "SELECT password_algo FROM users WHERE id = $1",
        user_id,
    )
    .await;
    let hash = scalar(
        &state,
        "SELECT password_hash FROM users WHERE id = $1",
        user_id,
    )
    .await;
    assert_eq!(algo, "argon2id");
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    assert!(!hash.contains("password123"));
}

#[tokio::test]
async fn email_phone_and_city_are_ciphertext_in_postgres() {
    let (base, state) = start_app().await;
    let login_name = unique_login();
    let (token, user_id) = register(&base, &login_name, "password123").await;
    let email = format!("{login_name}@openvk.example");
    let digits: String = login_name
        .chars()
        .filter(char::is_ascii_digit)
        .take(10)
        .collect();
    let phone = format!("+7 {digits}");

    let challenge = fetch_challenge(&base).await;
    let response = client()
        .put(format!("{base}/api/v1/settings"))
        .header("authorization", auth_header(&token))
        .headers(proto_headers(&challenge.csrf_token))
        .body(encode_pb(&pb::UpdateAccount {
            first_name: "Test".into(),
            last_name: "User".into(),
            email: Some(email.clone()),
            phone: Some(phone.clone()),
            city: Some("Novosibirsk".into()),
            privacy_wall: pb::PrivacyLevel::Everyone as i32,
            privacy_messages: pb::PrivacyLevel::Everyone as i32,
            privacy_photos: pb::PrivacyLevel::Everyone as i32,
            privacy_audio: pb::PrivacyLevel::Everyone as i32,
            privacy_profile: pb::PrivacyLevel::Everyone as i32,
            privacy_friends: pb::PrivacyLevel::Everyone as i32,
            status: None,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let body: pb::User = decode_response(response).await;
    assert_eq!(body.email.as_deref(), Some(email.as_str()));
    assert_eq!(body.phone.as_deref(), Some(phone.as_str()));
    assert_eq!(body.city.as_deref(), Some("Novosibirsk"));

    let stored_email =
        optional_scalar(&state, "SELECT email FROM users WHERE id = $1", user_id).await;
    let stored_phone =
        optional_scalar(&state, "SELECT phone FROM users WHERE id = $1", user_id).await;
    let stored_city =
        optional_scalar(&state, "SELECT city FROM users WHERE id = $1", user_id).await;
    assert!(stored_email.starts_with("ovk1."), "{stored_email}");
    assert!(stored_phone.starts_with("ovk1."), "{stored_phone}");
    assert!(stored_city.starts_with("ovk1."), "{stored_city}");
    assert!(!stored_email.contains(&email));
    assert!(!stored_phone.contains(&digits));
    assert!(!stored_city.contains("Novosibirsk"));

    let settings = client()
        .get(format!("{base}/api/v1/settings"))
        .header("authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    let settings: pb::User = decode_response(settings).await;
    assert_eq!(settings.email.as_deref(), Some(email.as_str()));
    assert_eq!(settings.city.as_deref(), Some("Novosibirsk"));
}

#[tokio::test]
async fn login_accepts_the_encrypted_email_address() {
    let (base, _) = start_app().await;
    let login_name = unique_login();
    let (token, _) = register(&base, &login_name, "password123").await;
    let email = format!("{login_name}@openvk.example");

    let challenge = fetch_challenge(&base).await;
    let saved = client()
        .put(format!("{base}/api/v1/settings"))
        .header("authorization", auth_header(&token))
        .headers(proto_headers(&challenge.csrf_token))
        .body(encode_pb(&pb::UpdateAccount {
            first_name: "Test".into(),
            last_name: "User".into(),
            email: Some(email.clone()),
            phone: None,
            city: None,
            privacy_wall: pb::PrivacyLevel::Everyone as i32,
            privacy_messages: pb::PrivacyLevel::Everyone as i32,
            privacy_photos: pb::PrivacyLevel::Everyone as i32,
            privacy_audio: pb::PrivacyLevel::Everyone as i32,
            privacy_profile: pb::PrivacyLevel::Everyone as i32,
            privacy_friends: pb::PrivacyLevel::Everyone as i32,
            status: None,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let response = login(&base, &email, "password123").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
}

#[tokio::test]
async fn demo_pii_is_sealed_and_hometown_is_ciphertext() {
    let (_, state) = start_app().await;
    let email = scalar_by_login(&state, "SELECT email FROM users WHERE login = $1", "id1").await;
    let phone = scalar_by_login(&state, "SELECT phone FROM users WHERE login = $1", "id1").await;
    let city = scalar_by_login(&state, "SELECT city FROM users WHERE login = $1", "id1").await;
    assert!(email.starts_with("ovk1."), "{email}");
    assert!(phone.starts_with("ovk1."), "{phone}");
    assert!(city.starts_with("ovk1."), "{city}");

    let wrap = scalar_by_login(&state, "SELECT wrap_key FROM users WHERE login = $1", "id1").await;
    assert!(wrap.starts_with("ovk1."), "{wrap}");

    let hometown = scalar_by_login(
        &state,
        "SELECT hometown FROM profiles p JOIN users u ON u.id = p.user_id WHERE u.login = $1",
        "id1",
    )
    .await;
    assert!(hometown.starts_with("ovk1."), "{hometown}");
    assert!(!hometown.contains("Saint Petersburg"));
}

#[tokio::test]
async fn messages_are_ciphertext_in_postgres_and_plaintext_on_the_api() {
    let (base, state) = start_app().await;
    let alice = unique_login();
    let bob = unique_login();
    let (alice_token, alice_id) = register(&base, &alice, "password123").await;
    let (bob_token, bob_id) = register(&base, &bob, "password123").await;
    let text = "secret guestbook note";

    let challenge = fetch_challenge(&base).await;
    let sent = client()
        .post(format!("{base}/api/v1/messages"))
        .header("authorization", auth_header(&alice_token))
        .headers(proto_headers(&challenge.csrf_token))
        .body(encode_pb(&pb::SendMessage {
            peer_id: bob_id,
            text: text.to_owned(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        sent.status(),
        StatusCode::OK,
        "{}",
        sent.text().await.unwrap()
    );
    let sent: pb::Message = decode_response(sent).await;
    assert_eq!(sent.text, text);
    assert_eq!(sent.author_id, alice_id);
    assert_eq!(sent.peer_id, bob_id);
    let message_id = sent.id;

    let stored = scalar(
        &state,
        "SELECT content FROM messages WHERE id = $1",
        message_id,
    )
    .await;
    assert!(stored.starts_with("ovk1."), "{stored}");
    assert!(!stored.contains(text));

    let listed = client()
        .get(format!("{base}/api/v1/messages?peer_id={alice_id}"))
        .header("authorization", auth_header(&bob_token))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: pb::MessageList = decode_response(listed).await;
    assert_eq!(listed.messages[0].text, text);
    assert_eq!(listed.messages[0].author_id, alice_id);
}

async fn scalar(state: &AppState, sql: &str, id: i64) -> String {
    optional_scalar(state, sql, id).await
}

async fn optional_scalar(state: &AppState, sql: &str, id: i64) -> String {
    let row = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [id.into()],
        ))
        .await
        .unwrap()
        .expect("row");
    row.try_get_by_index::<String>(0).unwrap_or_else(|_| {
        row.try_get_by_index::<Option<String>>(0)
            .ok()
            .flatten()
            .expect("text")
    })
}

async fn scalar_by_login(state: &AppState, sql: &str, login: &str) -> String {
    state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            [login.into()],
        ))
        .await
        .unwrap()
        .and_then(|row| row.try_get_by_index::<Option<String>>(0).ok())
        .flatten()
        .expect("row")
}
