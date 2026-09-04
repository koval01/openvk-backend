//! CSRF tokens, one-time login challenges, and RSA-OAEP password sealing.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use moka::future::Cache;
use rand::RngCore;
use rsa::pkcs8::{DecodePublicKey, EncodePublicKey};
use rsa::rand_core::OsRng;
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use serde::Serialize;
use sha2::Sha256;
use uuid::Uuid;

use crate::error::AppError;

const CSRF_TTL_SECS: u64 = 2 * 60 * 60;
const CHALLENGE_TTL: Duration = Duration::from_secs(120);
const RSA_BITS: usize = 2048;
const FIELD_SEP: u8 = 0;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct Security {
    csrf_secret: Vec<u8>,
    rsa: Arc<RsaPrivateKey>,
    public_spki_b64: String,
    challenges: Cache<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChallengeResponse {
    pub csrf_token: String,
    pub challenge_id: String,
    pub nonce: String,
    pub expires_in: u64,
    pub alg: &'static str,
    pub public_key: String,
}

impl Security {
    pub fn new(secret: &str) -> Result<Self, AppError> {
        let private = RsaPrivateKey::new(&mut OsRng, RSA_BITS)
            .map_err(|err| AppError::internal(format!("rsa keygen: {err}")))?;
        let public = RsaPublicKey::from(&private);
        let der = public
            .to_public_key_der()
            .map_err(|err| AppError::internal(format!("rsa spki: {err}")))?;
        Ok(Self {
            csrf_secret: secret.as_bytes().to_vec(),
            rsa: Arc::new(private),
            public_spki_b64: STANDARD.encode(der.as_bytes()),
            challenges: Cache::builder()
                .max_capacity(200_000)
                .time_to_live(CHALLENGE_TTL)
                .build(),
        })
    }

    #[must_use]
    pub fn public_key_spki(&self) -> &str {
        &self.public_spki_b64
    }

    #[must_use]
    pub fn issue_csrf(&self) -> String {
        let mut id = [0_u8; 16];
        rand::rng().fill_bytes(&mut id);
        let exp = unix_now().saturating_add(CSRF_TTL_SECS);
        let id_b64 = URL_SAFE_NO_PAD.encode(id);
        let mac = csrf_mac(&self.csrf_secret, &id_b64, exp);
        format!("v1.{id_b64}.{exp}.{mac}")
    }

    pub fn verify_csrf(&self, token: &str) -> Result<(), AppError> {
        let mut parts = token.split('.');
        let (Some("v1"), Some(id), Some(exp_raw), Some(mac), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(csrf_error());
        };
        let exp: u64 = exp_raw.parse().map_err(|_| csrf_error())?;
        if unix_now() > exp {
            return Err(csrf_error());
        }
        let expected = csrf_mac(&self.csrf_secret, id, exp);
        if !constant_eq(expected.as_bytes(), mac.as_bytes()) {
            return Err(csrf_error());
        }
        Ok(())
    }

    pub async fn issue_challenge(&self) -> ChallengeResponse {
        let challenge_id = Uuid::now_v7().to_string();
        let mut nonce = [0_u8; 16];
        rand::rng().fill_bytes(&mut nonce);
        let nonce = hex_encode(&nonce);
        self.challenges
            .insert(challenge_id.clone(), nonce.clone())
            .await;
        ChallengeResponse {
            csrf_token: self.issue_csrf(),
            challenge_id,
            nonce,
            expires_in: CHALLENGE_TTL.as_secs(),
            alg: "RSA-OAEP-256",
            public_key: self.public_spki_b64.clone(),
        }
    }

    pub async fn open_sealed(
        &self,
        challenge_id: &str,
        password_sealed: &str,
        expected_fields: usize,
    ) -> Result<Vec<String>, AppError> {
        let nonce = self.challenges.remove(challenge_id).await.ok_or_else(|| {
            AppError::Validation("login challenge expired or already used".into())
        })?;
        let ciphertext = STANDARD.decode(password_sealed.trim()).map_err(|_| {
            AppError::Validation("password must be sealed with the login challenge".into())
        })?;
        let padding = Oaep::new::<Sha256>();
        let plain = self
            .rsa
            .decrypt(padding, &ciphertext)
            .map_err(|_| AppError::Validation("could not open sealed password".into()))?;
        let fields = split_fields(&plain);
        if fields.len() != expected_fields + 1 || fields[0] != nonce {
            return Err(AppError::Validation(
                "sealed password does not match the login challenge".into(),
            ));
        }
        Ok(fields.into_iter().skip(1).collect())
    }
}

/// Encrypt `nonce` plus extra fields the same way the browser does (tests / tooling).
pub fn seal_fields(
    public_key_spki_b64: &str,
    nonce: &str,
    fields: &[&str],
) -> Result<String, AppError> {
    let der = STANDARD
        .decode(public_key_spki_b64.trim())
        .map_err(|err| AppError::internal(format!("spki b64: {err}")))?;
    let public = RsaPublicKey::from_public_key_der(&der)
        .map_err(|err| AppError::internal(format!("spki: {err}")))?;
    let mut plain = nonce.as_bytes().to_vec();
    for field in fields {
        plain.push(FIELD_SEP);
        plain.extend_from_slice(field.as_bytes());
    }
    let padding = Oaep::new::<Sha256>();
    let sealed = public
        .encrypt(&mut OsRng, padding, &plain)
        .map_err(|err| AppError::internal(format!("rsa seal: {err}")))?;
    Ok(STANDARD.encode(sealed))
}

fn csrf_mac(secret: &[u8], id: &str, exp: u64) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(b"v1");
    mac.update(id.as_bytes());
    mac.update(exp.to_string().as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub fn session_cookie(token: &str, secure: bool, max_age: u32) -> String {
    format_cookie("ovk_token", token, true, secure, max_age)
}

pub fn csrf_cookie(token: &str, secure: bool) -> String {
    format_cookie("ovk_csrf", token, true, secure, CSRF_TTL_SECS as u32)
}

pub fn clear_session_cookie(secure: bool) -> String {
    format_cookie("ovk_token", "", true, secure, 0)
}

pub fn clear_csrf_cookie(secure: bool) -> String {
    format_cookie("ovk_csrf", "", true, secure, 0)
}

fn format_cookie(name: &str, value: &str, http_only: bool, secure: bool, max_age: u32) -> String {
    let mut cookie = format!("{name}={value}; Path=/; SameSite=Lax; Max-Age={max_age}");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn csrf_error() -> AppError {
    AppError::Forbidden
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn split_fields(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == FIELD_SEP)
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn constant_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::{Security, seal_fields};

    #[tokio::test]
    async fn csrf_roundtrip() {
        let security = Security::new("test-secret").unwrap();
        let token = security.issue_csrf();
        security.verify_csrf(&token).unwrap();
        assert!(security.verify_csrf("nope").is_err());
    }

    #[tokio::test]
    async fn sealed_password_roundtrip() {
        let security = Security::new("test-secret").unwrap();
        let challenge = security.issue_challenge().await;
        let sealed =
            seal_fields(&challenge.public_key, &challenge.nonce, &["secret-pass"]).unwrap();
        let fields = security
            .open_sealed(&challenge.challenge_id, &sealed, 1)
            .await
            .unwrap();
        assert_eq!(fields, ["secret-pass"]);
        assert!(
            security
                .open_sealed(&challenge.challenge_id, &sealed, 1)
                .await
                .is_err()
        );
    }
}
