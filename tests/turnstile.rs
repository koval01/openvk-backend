use openvk_backend::{
    AppError, DUMMY_FAIL_SECRET, DUMMY_PASS_SECRET, DUMMY_SPENT_SECRET, DUMMY_TOKEN,
    SITEVERIFY_URL, Turnstile,
};

fn is_validation(error: &AppError) -> bool {
    error.to_string().starts_with("validation failed:")
}

#[tokio::test]
async fn dummy_pass_secret_accepts_the_dummy_token() {
    let turnstile = Turnstile::new(DUMMY_PASS_SECRET, SITEVERIFY_URL);
    turnstile
        .verify(DUMMY_TOKEN, Some("127.0.0.1"))
        .await
        .expect("always-pass dummy secret should accept XXXX.DUMMY.TOKEN.XXXX");
}

#[tokio::test]
async fn dummy_pass_secret_accepts_any_non_empty_token() {
    // Cloudflare's always-pass dummy secret returns success for any non-empty
    // response, not only XXXX.DUMMY.TOKEN.XXXX.
    let turnstile = Turnstile::new(DUMMY_PASS_SECRET, SITEVERIFY_URL);
    turnstile
        .verify("not-a-turnstile-token", None)
        .await
        .expect("always-pass dummy secret should accept any non-empty token");
}

#[tokio::test]
async fn dummy_fail_secret_rejects_the_dummy_token() {
    let turnstile = Turnstile::new(DUMMY_FAIL_SECRET, SITEVERIFY_URL);
    let error = turnstile.verify(DUMMY_TOKEN, None).await.unwrap_err();
    assert!(is_validation(&error), "{error}");
    assert!(error.to_string().contains("human"), "{error}");
}

#[tokio::test]
async fn dummy_spent_secret_asks_the_user_to_retry() {
    let turnstile = Turnstile::new(DUMMY_SPENT_SECRET, SITEVERIFY_URL);
    let error = turnstile.verify(DUMMY_TOKEN, None).await.unwrap_err();
    assert!(is_validation(&error), "{error}");
    assert!(error.to_string().contains("try again"), "{error}");
}
