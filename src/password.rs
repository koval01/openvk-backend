//! Password hashing. New hashes are Argon2id (PHC). Legacy SHA-256 still verifies.

use std::env;
use std::sync::OnceLock;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use sha2::{Digest, Sha256};

use crate::error::AppError;

pub const ALGO: &str = "argon2id";

const LEGACY_SALT: &[u8] = b"openvk-dev-salt:";

pub fn hash(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    hasher()?
        .hash_password(password.as_bytes(), &salt)
        .map(|hashed| hashed.to_string())
        .map_err(|err| AppError::internal(format!("argon2 hash: {err}")))
}

#[must_use]
pub fn verify(password: &str, password_hash: &str) -> bool {
    if password_hash.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(password_hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    } else {
        legacy_sha256(password) == password_hash
    }
}

#[must_use]
pub fn needs_rehash(password_hash: &str) -> bool {
    !password_hash.starts_with("$argon2")
}

fn hasher() -> Result<Argon2<'static>, AppError> {
    static HASHER: OnceLock<Argon2<'static>> = OnceLock::new();
    if let Some(hasher) = HASHER.get() {
        return Ok(hasher.clone());
    }
    let hasher = build_hasher()?;
    Ok(HASHER.get_or_init(|| hasher).clone())
}

fn build_hasher() -> Result<Argon2<'static>, AppError> {
    let m_kib = env_u32("ARGON2_M_KIB", 8_192);
    let t_cost = env_u32("ARGON2_T_COST", 2);
    let p_cost = env_u32("ARGON2_P_COST", 1);
    let params = Params::new(m_kib, t_cost, p_cost, None)
        .map_err(|err| AppError::internal(format!("argon2 params: {err}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn legacy_sha256(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(LEGACY_SALT);
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{hash, legacy_sha256, needs_rehash, verify};

    #[test]
    fn argon2_hashes_verify_and_are_unique() {
        let hashed = hash("openvk").expect("hash");
        assert!(hashed.starts_with("$argon2id$"));
        assert!(verify("openvk", &hashed));
        assert!(!verify("other", &hashed));
        assert!(!needs_rehash(&hashed));
        assert_ne!(hashed, hash("openvk").expect("hash again"));
    }

    #[test]
    fn legacy_sha256_still_verifies() {
        let hashed = legacy_sha256("openvk");
        assert_eq!(hashed.len(), 64);
        assert!(verify("openvk", &hashed));
        assert!(!verify("other", &hashed));
        assert!(needs_rehash(&hashed));
    }
}
