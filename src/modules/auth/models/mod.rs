use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Deserialize)]
pub struct Credentials {
    pub login: String,
    pub challenge_id: String,
    pub password_sealed: String,
    #[serde(
        default,
        alias = "cf-turnstile-response",
        alias = "cf_turnstile_response"
    )]
    pub turnstile_token: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub token_type: &'static str,
    pub user_id: i64,
}
