use serde::Deserialize;

use crate::error::AppError;

pub const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
pub const DUMMY_PASS_SECRET: &str = "1x0000000000000000000000000000000AA";
pub const DUMMY_FAIL_SECRET: &str = "2x0000000000000000000000000000000AA";
pub const DUMMY_SPENT_SECRET: &str = "3x0000000000000000000000000000000AA";
pub const DUMMY_TOKEN: &str = "XXXX.DUMMY.TOKEN.XXXX";

#[derive(Clone)]
pub struct Turnstile {
    client: reqwest::Client,
    secret: String,
    siteverify_url: String,
}

#[derive(Debug, Deserialize)]
struct SiteverifyResponse {
    success: bool,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
}

impl Turnstile {
    #[must_use]
    pub fn new(secret: impl Into<String>, siteverify_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            secret: secret.into(),
            siteverify_url: siteverify_url.into(),
        }
    }

    pub async fn verify(&self, token: &str, remote_ip: Option<&str>) -> Result<(), AppError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::Validation("complete the security check".into()));
        }

        // Dummy always-pass secrets accept any non-empty token. Empty tokens are
        // rejected here so login/register cannot skip the widget.

        let mut form = vec![
            ("secret".to_owned(), self.secret.clone()),
            ("response".to_owned(), token.to_owned()),
        ];
        if let Some(ip) = remote_ip.map(str::trim).filter(|value| !value.is_empty()) {
            form.push(("remoteip".to_owned(), ip.to_owned()));
        }

        let response = self
            .client
            .post(&self.siteverify_url)
            .form(&form)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("turnstile siteverify failed: {err}")))?;
        let status = response.status();
        let body = response.json::<SiteverifyResponse>().await.map_err(|err| {
            AppError::internal(format!("turnstile siteverify was not json: {err}"))
        })?;

        if body.success {
            return Ok(());
        }

        tracing::debug!(
            %status,
            errors = ?body.error_codes,
            "turnstile rejected the token"
        );
        Err(AppError::Validation(message_for(&body.error_codes)))
    }
}

fn message_for(codes: &[String]) -> String {
    if codes.iter().any(|code| code == "timeout-or-duplicate") {
        return "security check expired — try again".into();
    }
    if codes
        .iter()
        .any(|code| code == "missing-input-secret" || code == "invalid-input-secret")
    {
        return "security check is not configured".into();
    }
    "could not verify you are human".into()
}

#[cfg(test)]
mod tests {
    use super::{Turnstile, message_for};

    #[tokio::test]
    async fn empty_token_is_rejected_without_a_network_call() {
        let turnstile = Turnstile::new("secret", "http://127.0.0.1:1/siteverify");
        let error = turnstile.verify("  ", None).await.unwrap_err();
        assert!(error.to_string().contains("security check"));
    }

    #[test]
    fn maps_spent_tokens_to_a_retry_message() {
        assert!(message_for(&["timeout-or-duplicate".into()]).contains("try again"));
        assert!(message_for(&["invalid-input-response".into()]).contains("human"));
    }
}
