mod common;

use common::{
    decode_error, decode_response, login, proto_headers, register, start_app, unique_login,
};
use openvk_backend::{encode_pb, pb};
use reqwest::StatusCode;

async fn session(base: &str, login_name: &str, password: &str) -> (String, i64) {
    let response = login(base, login_name, password).await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", response.text().await.unwrap());
    let body: pb::Token = decode_response(response).await;
    (body.token, body.user_id)
}

fn auth_headers(token: &str, csrf: &str) -> reqwest::header::HeaderMap {
    let mut headers = proto_headers(csrf);
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

#[tokio::test]
async fn gifts_vouchers_tickets_reports_and_admin_close() {
    let (base, _) = start_app().await;
    let client = reqwest::Client::new();
    let csrf = common::fetch_challenge(&base).await.csrf_token;
    let (admin_token, admin_id) = session(&base, "id1", "openvk").await;
    let (user_token, user_id) = register(&base, &unique_login(), "password123").await;

    let overview = client
        .get(format!("{base}/api/v1/admin/overview"))
        .headers(auth_headers(&admin_token, &csrf))
        .send()
        .await
        .unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let overview: pb::AdminOverview = decode_response(overview).await;
    assert!(overview.users >= 3, "{}", overview.users);

    let catalog = client
        .get(format!("{base}/api/v1/gifts"))
        .headers(auth_headers(&admin_token, &csrf))
        .send()
        .await
        .unwrap();
    assert_eq!(catalog.status(), StatusCode::OK);
    let catalog: pb::GiftCatalog = decode_response(catalog).await;
    assert!(!catalog.categories.is_empty());
    let gift_id = catalog.categories[0].gifts[0].id;

    let sent = client
        .post(format!("{base}/api/v1/gifts"))
        .headers(auth_headers(&admin_token, &csrf))
        .body(encode_pb(&pb::SendGift {
            gift_id,
            receiver_id: user_id,
            caption: Some("hello".into()),
            anonymous: false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        sent.status(),
        StatusCode::CREATED,
        "{}",
        sent.text().await.unwrap()
    );
    let received = client
        .get(format!("{base}/api/v1/users/{user_id}/gifts"))
        .headers(auth_headers(&user_token, &csrf))
        .send()
        .await
        .unwrap();
    let gifts: pb::UserGiftList = decode_response(received).await;
    assert_eq!(gifts.gifts.len(), 1);
    assert_eq!(gifts.gifts[0].sender_id, admin_id);

    let redeemed = client
        .post(format!("{base}/api/v1/vouchers/redeem"))
        .headers(auth_headers(&user_token, &csrf))
        .body(encode_pb(&pb::RedeemVoucher {
            serial: "AAAAAA-BBBBBB-CCCCCC-DDDDDD".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        redeemed.status(),
        StatusCode::OK,
        "{}",
        redeemed.text().await.unwrap()
    );
    let funded: pb::User = decode_response(redeemed).await;
    assert!(funded.coins >= 25, "{}", funded.coins);

    let ticket = client
        .post(format!("{base}/api/v1/tickets"))
        .headers(auth_headers(&user_token, &csrf))
        .body(encode_pb(&pb::WriteTicket {
            subject: "Help".into(),
            content: "The wall is a guestbook.".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ticket.status(), StatusCode::CREATED);
    let ticket: pb::Ticket = decode_response(ticket).await;
    assert_eq!(ticket.status, "open");

    let reply = client
        .post(format!("{base}/api/v1/tickets/{}/replies", ticket.id))
        .headers(auth_headers(&admin_token, &csrf))
        .body(encode_pb(&pb::WriteTicketReply {
            content: "We will look into it.".into(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    let reply: pb::Ticket = decode_response(reply).await;
    assert_eq!(reply.replies.len(), 1);
    assert!(reply.replies[0].from_agent);

    let report = client
        .post(format!("{base}/api/v1/reports"))
        .headers(auth_headers(&user_token, &csrf))
        .body(encode_pb(&pb::WriteReport {
            target_type: "user".into(),
            target_id: admin_id,
            reason: "test".into(),
            owner_id: None,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(report.status(), StatusCode::CREATED);
    let reports = client
        .get(format!("{base}/api/v1/reports"))
        .headers(auth_headers(&admin_token, &csrf))
        .send()
        .await
        .unwrap();
    let reports: pb::ReportList = decode_response(reports).await;
    assert!(!reports.reports.is_empty());
}

#[tokio::test]
async fn bans_away_nospam_and_vk_method() {
    let (base, _) = start_app().await;
    let client = reqwest::Client::new();
    let csrf = common::fetch_challenge(&base).await.csrf_token;
    let (admin_token, _) = session(&base, "id1", "openvk").await;
    let login_name = unique_login();
    let (user_token, user_id) = register(&base, &login_name, "password123").await;

    let banned = client
        .post(format!("{base}/api/v1/admin/users/{user_id}/ban"))
        .headers(auth_headers(&admin_token, &csrf))
        .body(encode_pb(&pb::BanUser {
            reason: "spam".into(),
            until: None,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        banned.status(),
        StatusCode::OK,
        "{}",
        banned.text().await.unwrap()
    );

    let denied = login(&base, &login_name, "password123").await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let error = decode_error(denied).await;
    assert_eq!(error.error, "banned");
    assert!(error.message.contains("spam"), "{}", error.message);

    let unbanned = client
        .post(format!("{base}/api/v1/admin/users/{user_id}/unban"))
        .headers(auth_headers(&admin_token, &csrf))
        .body(Vec::new())
        .send()
        .await;
    drop(user_token);
    let unbanned = unbanned.unwrap();
    assert_eq!(
        unbanned.status(),
        StatusCode::OK,
        "{}",
        unbanned.text().await.unwrap()
    );

    let wall = client
        .post(format!("{base}/api/v1/users/{user_id}/wall"))
        .headers(auth_headers(&admin_token, &csrf))
        .body(encode_pb(&pb::WriteWall {
            content: "nospam-unique-token-xyz".into(),
            ..Default::default()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(wall.status(), StatusCode::CREATED);

    let nospam = client
        .post(format!("{base}/api/v1/nospam"))
        .headers(auth_headers(&admin_token, &csrf))
        .body(encode_pb(&pb::NospamQuery {
            query: "nospam-unique-token-xyz".into(),
            delete_hits: true,
            ban_authors: false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        nospam.status(),
        StatusCode::OK,
        "{}",
        nospam.text().await.unwrap()
    );
    let nospam: pb::NospamResult = decode_response(nospam).await;
    assert_eq!(nospam.deleted, 1);

    let link = client
        .get(format!("{base}/api/v1/away/links/1"))
        .send()
        .await
        .unwrap();
    assert_eq!(link.status(), StatusCode::OK);
    let link: pb::BannedLink = decode_response(link).await;
    assert!(link.url.contains("evil.example"));

    let check = client
        .get(format!(
            "{base}/api/v1/away/check?url=https://evil.example/spam"
        ))
        .send()
        .await
        .unwrap();
    let check: pb::BannedLinkList = decode_response(check).await;
    assert_eq!(check.links.len(), 1);

    let vk = client
        .get(format!(
            "{base}/method/users.get?access_token={admin_token}&user_ids={user_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(vk.status(), StatusCode::OK);
    let body: serde_json::Value = vk.json().await.unwrap();
    assert!(body.get("response").is_some(), "{body}");
    assert!(body.get("error").is_none(), "{body}");

    let token = client
        .post(format!("{base}/token"))
        .form(&[
            ("username", "id1"),
            ("password", "openvk"),
            ("grant_type", "password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        token.status(),
        StatusCode::OK,
        "{}",
        token.text().await.unwrap()
    );
    let token: serde_json::Value = token.json().await.unwrap();
    assert!(token.get("access_token").is_some(), "{token}");
}
