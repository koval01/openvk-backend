//! Application-level encryption at rest.
//!
//! A stolen database dump holds Argon2 hashes, AES-GCM blobs, and HMAC blind
//! indexes. Those are useless without `DATA_KEY` on the API instance. A stolen
//! instance holds code and `DATA_KEY`, which are useless without the rows.
//! A live process that can reach both can still decrypt — this is not E2E.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::AppError;

const PREFIX: &str = "ovk1.";
const NONCE_LEN: usize = 12;
const HKDF_SALT: &[u8] = b"openvk-vault-v1";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct Vault {
    pii: Aes256Gcm,
    wrap: Aes256Gcm,
    idx_key: [u8; 32],
}

impl Vault {
    pub fn from_secret(secret: &str) -> Result<Self, AppError> {
        let master = parse_master_key(secret);
        let pii = Aes256Gcm::new_from_slice(&derive(&master, b"pii-enc")?)
            .map_err(|err| AppError::internal(format!("pii cipher: {err}")))?;
        let wrap = Aes256Gcm::new_from_slice(&derive(&master, b"user-wrap")?)
            .map_err(|err| AppError::internal(format!("wrap cipher: {err}")))?;
        Ok(Self {
            pii,
            wrap,
            idx_key: derive(&master, b"pii-idx")?,
        })
    }

    #[must_use]
    pub fn is_sealed(value: &str) -> bool {
        value.starts_with(PREFIX)
    }

    #[must_use]
    pub fn random_key() -> [u8; 32] {
        let mut key = [0_u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    pub fn encrypt_field(&self, aad: &str, plaintext: &str) -> Result<String, AppError> {
        seal(&self.pii, aad.as_bytes(), plaintext.as_bytes())
    }

    pub fn decrypt_field(&self, aad: &str, stored: &str) -> Result<String, AppError> {
        let bytes = open(&self.pii, aad.as_bytes(), stored)?;
        String::from_utf8(bytes).map_err(|_| AppError::internal("sealed field is not utf-8"))
    }

    pub fn maybe_decrypt(&self, aad: &str, stored: &str) -> Result<String, AppError> {
        if Self::is_sealed(stored) {
            self.decrypt_field(aad, stored)
        } else {
            Ok(stored.to_owned())
        }
    }

    pub fn maybe_decrypt_opt(
        &self,
        aad: &str,
        stored: Option<String>,
    ) -> Result<Option<String>, AppError> {
        stored
            .map(|value| self.maybe_decrypt(aad, &value))
            .transpose()
    }

    pub fn encrypt_opt(
        &self,
        aad: &str,
        plaintext: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        plaintext
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| self.encrypt_field(aad, value))
            .transpose()
    }

    #[must_use]
    pub fn blind_index(&self, purpose: &str, value: &str) -> String {
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(&self.idx_key).expect("hmac key length is valid");
        mac.update(purpose.as_bytes());
        mac.update(b":");
        mac.update(value.as_bytes());
        hex_lower(mac.finalize().into_bytes())
    }

    pub fn wrap_user_key(&self, user_id: i64, key: &[u8; 32]) -> Result<String, AppError> {
        seal(&self.wrap, aad_user_wrap(user_id).as_bytes(), key)
    }

    pub fn unwrap_user_key(&self, user_id: i64, stored: &str) -> Result<[u8; 32], AppError> {
        let bytes = open(&self.wrap, aad_user_wrap(user_id).as_bytes(), stored)?;
        bytes
            .try_into()
            .map_err(|_| AppError::internal("user wrap key length"))
    }

    pub fn wrap_conversation_dek(
        member_wrap: &[u8; 32],
        conversation_id: i64,
        user_id: i64,
        dek: &[u8; 32],
    ) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(member_wrap)
            .map_err(|err| AppError::internal(format!("member wrap cipher: {err}")))?;
        seal(
            &cipher,
            aad_conv_dek(conversation_id, user_id).as_bytes(),
            dek,
        )
    }

    pub fn unwrap_conversation_dek(
        member_wrap: &[u8; 32],
        conversation_id: i64,
        user_id: i64,
        stored: &str,
    ) -> Result<[u8; 32], AppError> {
        let cipher = Aes256Gcm::new_from_slice(member_wrap)
            .map_err(|err| AppError::internal(format!("member wrap cipher: {err}")))?;
        let bytes = open(
            &cipher,
            aad_conv_dek(conversation_id, user_id).as_bytes(),
            stored,
        )?;
        bytes
            .try_into()
            .map_err(|_| AppError::internal("conversation dek length"))
    }

    pub fn encrypt_message(
        dek: &[u8; 32],
        conversation_id: i64,
        plaintext: &str,
    ) -> Result<String, AppError> {
        let cipher = Aes256Gcm::new_from_slice(dek)
            .map_err(|err| AppError::internal(format!("message cipher: {err}")))?;
        seal(
            &cipher,
            aad_msg(conversation_id).as_bytes(),
            plaintext.as_bytes(),
        )
    }

    pub fn decrypt_message(
        dek: &[u8; 32],
        conversation_id: i64,
        stored: &str,
    ) -> Result<String, AppError> {
        if !Self::is_sealed(stored) {
            return Ok(stored.to_owned());
        }
        let cipher = Aes256Gcm::new_from_slice(dek)
            .map_err(|err| AppError::internal(format!("message cipher: {err}")))?;
        let bytes = open(&cipher, aad_msg(conversation_id).as_bytes(), stored)?;
        String::from_utf8(bytes).map_err(|_| AppError::internal("message is not utf-8"))
    }
}

#[must_use]
pub fn aad_email(user_id: i64) -> String {
    format!("email:{user_id}")
}

#[must_use]
pub fn aad_phone(user_id: i64) -> String {
    format!("phone:{user_id}")
}

#[must_use]
pub fn aad_city(user_id: i64) -> String {
    format!("city:{user_id}")
}

#[must_use]
pub fn aad_hometown(user_id: i64) -> String {
    format!("hometown:{user_id}")
}

#[must_use]
pub fn aad_birthday(user_id: i64) -> String {
    format!("birthday:{user_id}")
}

#[must_use]
pub fn aad_address(user_id: i64) -> String {
    format!("address:{user_id}")
}

#[must_use]
pub fn aad_contact_email(user_id: i64) -> String {
    format!("contact_email:{user_id}")
}

#[must_use]
pub fn aad_user_wrap(user_id: i64) -> String {
    format!("user-wrap:{user_id}")
}

#[must_use]
pub fn aad_conv_dek(conversation_id: i64, user_id: i64) -> String {
    format!("conv-dek:{conversation_id}:{user_id}")
}

#[must_use]
pub fn aad_msg(conversation_id: i64) -> String {
    format!("msg:{conversation_id}")
}

#[must_use]
pub fn aad_cache_user(user_id: i64) -> String {
    format!("cache:user:{user_id}")
}

#[must_use]
pub fn normalize_email(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[must_use]
pub fn normalize_phone(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '+')
        .collect()
}

fn parse_master_key(secret: &str) -> [u8; 32] {
    let trimmed = secret.trim();
    if let Ok(bytes) = STANDARD.decode(trimmed)
        && bytes.len() == 32
    {
        return bytes.try_into().expect("base64 length already checked");
    }
    if let Some(bytes) = decode_hex32(trimmed) {
        return bytes;
    }
    Sha256::digest(trimmed.as_bytes()).into()
}

fn decode_hex32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0_u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn derive(master: &[u8; 32], info: &[u8]) -> Result<[u8; 32], AppError> {
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), master);
    let mut okm = [0_u8; 32];
    hk.expand(info, &mut okm)
        .map_err(|err| AppError::internal(format!("hkdf: {err}")))?;
    Ok(okm)
}

fn seal(cipher: &Aes256Gcm, aad: &[u8], plaintext: &[u8]) -> Result<String, AppError> {
    let mut nonce = [0_u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::internal("encrypt failed"))?;
    let mut packed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    packed.extend_from_slice(&nonce);
    packed.extend_from_slice(&ciphertext);
    Ok(format!("{PREFIX}{}", STANDARD.encode(packed)))
}

fn open(cipher: &Aes256Gcm, aad: &[u8], stored: &str) -> Result<Vec<u8>, AppError> {
    let encoded = stored
        .strip_prefix(PREFIX)
        .ok_or_else(|| AppError::internal("sealed field prefix"))?;
    let packed = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::internal("sealed field encoding"))?;
    if packed.len() < NONCE_LEN + 16 {
        return Err(AppError::internal("sealed field truncated"));
    }
    let (nonce, ciphertext) = packed.split_at(NONCE_LEN);
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::internal("decrypt failed"))
}

fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::Vault;

    #[test]
    fn roundtrip_and_aad_binding() {
        let vault = Vault::from_secret("openvk-dev-data-key-change-me").expect("vault");
        let sealed = vault
            .encrypt_field("email:1", "ivan@openvk.local")
            .expect("seal");
        assert!(Vault::is_sealed(&sealed));
        assert_eq!(
            vault.maybe_decrypt("email:1", &sealed).expect("open"),
            "ivan@openvk.local"
        );
        assert!(vault.decrypt_field("email:2", &sealed).is_err());
        assert_eq!(
            vault
                .maybe_decrypt("email:1", "legacy@openvk.local")
                .expect("plain"),
            "legacy@openvk.local"
        );
    }

    #[test]
    fn conversation_dek_is_independent_of_instance_pii_key() {
        let wrap = Vault::random_key();
        let dek = Vault::random_key();
        let wrapped = Vault::wrap_conversation_dek(&wrap, 9, 3, &dek).expect("wrap");
        let opened = Vault::unwrap_conversation_dek(&wrap, 9, 3, &wrapped).expect("unwrap");
        assert_eq!(opened, dek);
        assert!(Vault::unwrap_conversation_dek(&wrap, 9, 4, &wrapped).is_err());
        let text = Vault::encrypt_message(&dek, 9, "hello").expect("msg");
        assert!(Vault::is_sealed(&text));
        assert_eq!(
            Vault::decrypt_message(&dek, 9, &text).expect("plain"),
            "hello"
        );
    }

    #[test]
    fn blind_indexes_are_stable_and_purpose_scoped() {
        let vault = Vault::from_secret("openvk-dev-data-key-change-me").expect("vault");
        let one = vault.blind_index("email", "ivan@openvk.local");
        let two = vault.blind_index("email", "ivan@openvk.local");
        assert_eq!(one, two);
        assert_eq!(one.len(), 64);
        assert_ne!(one, vault.blind_index("phone", "ivan@openvk.local"));
    }

    #[test]
    fn passphrase_and_sha256_hex_resolve_to_the_same_master() {
        use sha2::{Digest, Sha256};
        let phrase = "openvk-dev-data-key-change-me";
        let hex = format!("{:x}", Sha256::digest(phrase.as_bytes()));
        let a = Vault::from_secret(phrase).expect("phrase");
        let b = Vault::from_secret(&hex).expect("hex");
        let sealed = a.encrypt_field("email:1", "x").expect("seal");
        assert_eq!(b.decrypt_field("email:1", &sealed).expect("open"), "x");
    }
}
