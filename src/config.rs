use std::env;
use std::fs;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageBackend {
    Memory,
    Disk,
    R2,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub data_key: String,
    pub cors_origins: Vec<String>,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub rate_limit_per_minute: NonZeroU32,
    pub storage_backend: StorageBackend,
    pub media_root: PathBuf,
    pub cloudflare_account_id: Option<String>,
    pub cloudflare_api_token: Option<String>,
    pub r2_bucket: Option<String>,
    pub max_upload_bytes: u64,
    pub turnstile_secret_key: String,
    pub turnstile_siteverify_url: String,
    pub cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let host = env_or("HOST", "0.0.0.0");
        let port = env_parse("PORT", 8080_u16)?;
        let listen_addr = SocketAddr::from_str(&format!("{host}:{port}"))
            .map_err(|err| AppError::Config(format!("invalid HOST/PORT: {err}")))?;

        let cloudflare_account_id = env_optional("CLOUDFLARE_ACCOUNT_ID");
        let r2_bucket = env_optional("R2_BUCKET").or_else(|| env_optional("S3_BUCKET"));
        let cloudflare_api_token =
            env_optional("CLOUDFLARE_API_TOKEN").or_else(read_wrangler_oauth_token);
        let requested_backend = env_or("STORAGE_BACKEND", "disk").to_ascii_lowercase();
        let storage_backend = match requested_backend.as_str() {
            "memory" => StorageBackend::Memory,
            "disk" | "filesystem" | "local" | "auto" => StorageBackend::Disk,
            "r2" | "s3" => StorageBackend::R2,
            other => {
                return Err(AppError::Config(format!(
                    "invalid STORAGE_BACKEND '{other}', expected disk, memory, or r2"
                )));
            }
        };

        if storage_backend == StorageBackend::R2
            && (r2_bucket.is_none()
                || cloudflare_account_id.is_none()
                || cloudflare_api_token.is_none())
        {
            return Err(AppError::Config(
                "R2 storage requires CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_TOKEN, and R2_BUCKET"
                    .into(),
            ));
        }

        Ok(Self {
            listen_addr,
            database_url: env_or(
                "DATABASE_URL",
                "postgres://openvk:openvk@127.0.0.1:5432/openvk?sslmode=disable",
            ),
            redis_url: env_or("REDIS_URL", "redis://127.0.0.1:6379/"),
            jwt_secret: env_or("JWT_SECRET", "dev-only-change-me-to-a-long-random-string"),
            data_key: env_or("DATA_KEY", "openvk-dev-data-key-change-me"),
            cors_origins: expand_loopback_origins(
                env_or("CORS_ORIGIN", "http://127.0.0.1:5173")
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
            ),
            db_max_connections: env_parse("DB_MAX_CONNECTIONS", 32_u32)?,
            db_min_connections: env_parse("DB_MIN_CONNECTIONS", 2_u32)?,
            rate_limit_per_minute: NonZeroU32::new(env_parse("RATE_LIMIT_PER_MINUTE", 120_u32)?)
                .unwrap_or(NonZeroU32::MIN),
            storage_backend,
            media_root: env_optional("MEDIA_ROOT")
                .map_or_else(|| PathBuf::from("data/media"), PathBuf::from),
            cloudflare_account_id,
            cloudflare_api_token,
            r2_bucket,
            max_upload_bytes: env_parse("MAX_UPLOAD_BYTES", 52_428_800_u64)?,
            turnstile_secret_key: env_or(
                "TURNSTILE_SECRET_KEY",
                crate::turnstile::DUMMY_PASS_SECRET,
            ),
            turnstile_siteverify_url: env_or(
                "TURNSTILE_SITEVERIFY_URL",
                crate::turnstile::SITEVERIFY_URL,
            ),
            cookie_secure: matches!(
                env_or("COOKIE_SECURE", "0").to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            ),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// `localhost`, `127.0.0.1`, and `[::1]` are the same browser machine.
/// Browsers send the host as typed, so a CORS list with only 127.0.0.1
/// would 403 logins from http://localhost:5173.
fn expand_loopback_origins(origins: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut expanded = Vec::new();
    for origin in origins {
        for alias in loopback_aliases(&origin) {
            if !expanded.contains(&alias) {
                expanded.push(alias);
            }
        }
    }
    expanded
}

fn loopback_aliases(origin: &str) -> Vec<String> {
    const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];
    for scheme in ["http://", "https://"] {
        for host in LOOPBACK_HOSTS {
            let prefix = format!("{scheme}{host}");
            if origin == prefix || origin.starts_with(&format!("{prefix}:")) {
                let suffix = &origin[prefix.len()..];
                return LOOPBACK_HOSTS
                    .into_iter()
                    .map(|alias| format!("{scheme}{alias}{suffix}"))
                    .collect();
            }
        }
    }
    vec![origin.to_owned()]
}

fn env_optional(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_parse<T>(key: &str, default: T) -> Result<T, AppError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|err| AppError::Config(format!("invalid {key}: {err}"))),
        Err(_) => Ok(default),
    }
}

fn read_wrangler_oauth_token() -> Option<String> {
    let home = env::var("HOME").ok().map(PathBuf::from)?;
    let candidates = [
        home.join("Library/Preferences/.wrangler/config/default.toml"),
        home.join(".config/.wrangler/config/default.toml"),
        home.join(".wrangler/config/default.toml"),
    ];
    for path in candidates {
        if let Ok(text) = fs::read_to_string(path)
            && let Some(token) = toml_string_value(&text, "oauth_token")
        {
            return Some(token);
        }
    }
    None
}

fn toml_string_value(source: &str, key: &str) -> Option<String> {
    for line in source.lines() {
        let line = line.trim();
        let Some((found_key, value)) = line.split_once('=') else {
            continue;
        };
        if found_key.trim() != key {
            continue;
        }
        let value = value.trim().trim_matches('"').trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{StorageBackend, toml_string_value};

    #[test]
    fn parses_wrangler_oauth_token() {
        let source = "oauth_token = \"abc.def\"\nexpiration_time = \"2026-01-01\"";
        assert_eq!(
            toml_string_value(source, "oauth_token").as_deref(),
            Some("abc.def")
        );
    }

    #[test]
    fn storage_backend_equality() {
        assert_eq!(StorageBackend::R2, StorageBackend::R2);
        assert_ne!(StorageBackend::R2, StorageBackend::Disk);
    }

    #[test]
    fn loopback_cors_aliases_localhost() {
        let expanded = super::expand_loopback_origins(["http://127.0.0.1:5173".to_owned()]);
        assert!(expanded.contains(&"http://127.0.0.1:5173".to_owned()));
        assert!(expanded.contains(&"http://localhost:5173".to_owned()));
        assert!(expanded.contains(&"http://[::1]:5173".to_owned()));
        assert!(!expanded.iter().any(|origin| origin.contains("evil")));
    }
}
