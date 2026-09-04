use std::env;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::str::FromStr;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageBackend {
    /// Process memory. Nothing survives a restart; local development only.
    Memory,
    /// Any S3 API: Cloudflare R2, Silo, `MinIO`.
    S3,
}

/// Credentials and addressing for the S3 API.
/// R2 wants virtual-hosted requests, Silo and `MinIO` want path style.
#[derive(Clone, Debug)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub path_style: bool,
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
    pub s3: Option<S3Config>,
    /// Where the public reads objects: the nginx or CDN host in front of the bucket.
    pub media_public_base_url: String,
    pub max_upload_bytes: u64,
    pub turnstile_secret_key: String,
    pub turnstile_siteverify_url: String,
    pub cookie_secure: bool,
    /// `DiceBear` template with `{seed}`. Empty / `off` skips generated avatars.
    pub dicebear_url: Option<String>,
}

/// Notionists SVG. `{seed}` is replaced with the user id; the file is stored as `WebP`.
pub(crate) const DICEBEAR_DEFAULT_URL: &str = "https://api.dicebear.com/10.x/notionists/svg?backgroundColor=ececed&inkColor=3b3d42&paperColor=fafafa&seed={seed}";

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let host = env_or("HOST", "0.0.0.0");
        let port = env_parse("PORT", 8080_u16)?;
        let listen_addr = SocketAddr::from_str(&format!("{host}:{port}"))
            .map_err(|err| AppError::Config(format!("invalid HOST/PORT: {err}")))?;

        let s3 = s3_from_env()?;
        let requested_backend = env_optional("STORAGE_BACKEND")
            .unwrap_or_else(|| if s3.is_some() { "s3" } else { "memory" }.to_owned())
            .to_ascii_lowercase();
        let storage_backend = match requested_backend.as_str() {
            "memory" => StorageBackend::Memory,
            "s3" | "r2" | "silo" | "minio" => StorageBackend::S3,
            other => {
                return Err(AppError::Config(format!(
                    "invalid STORAGE_BACKEND '{other}', expected s3 or memory"
                )));
            }
        };

        if storage_backend == StorageBackend::S3 && s3.is_none() {
            return Err(AppError::Config(
                "S3 storage requires S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY"
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
            media_public_base_url: media_public_base_url(s3.as_ref()),
            s3,
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
            dicebear_url: dicebear_url_from_env(),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

impl S3Config {
    /// The bucket's own address, used when no nginx or CDN sits in front of it.
    fn direct_base_url(&self) -> String {
        if self.path_style {
            return format!("{}/{}", self.endpoint, self.bucket);
        }
        match self.endpoint.split_once("://") {
            Some((scheme, host)) => format!("{scheme}://{}.{host}", self.bucket),
            None => format!("{}/{}", self.endpoint, self.bucket),
        }
    }
}

/// Present only when the whole credential set is there. A half-filled S3 block is
/// an error rather than a silent fallback: uploads would vanish into memory.
fn s3_from_env() -> Result<Option<S3Config>, AppError> {
    let endpoint = env_optional("S3_ENDPOINT");
    let bucket = env_optional("S3_BUCKET").or_else(|| env_optional("R2_BUCKET"));
    let access_key_id = env_optional("S3_ACCESS_KEY_ID").or_else(|| env_optional("S3_ACCESS_KEY"));
    let secret_access_key =
        env_optional("S3_SECRET_ACCESS_KEY").or_else(|| env_optional("S3_SECRET_KEY"));

    let provided = [&endpoint, &bucket, &access_key_id, &secret_access_key]
        .into_iter()
        .filter(|value| value.is_some())
        .count();
    if provided == 0 {
        return Ok(None);
    }

    let (Some(endpoint), Some(bucket), Some(access_key_id), Some(secret_access_key)) =
        (endpoint, bucket, access_key_id, secret_access_key)
    else {
        return Err(AppError::Config(
            "incomplete S3 configuration: S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY are all required".into(),
        ));
    };

    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return Err(AppError::Config(format!(
            "S3_ENDPOINT must start with http:// or https://, got '{endpoint}'"
        )));
    }

    Ok(Some(S3Config {
        endpoint: endpoint.trim_end_matches('/').to_owned(),
        bucket,
        region: env_or("S3_REGION", "auto"),
        access_key_id,
        secret_access_key,
        path_style: env_flag("S3_PATH_STYLE", true),
    }))
}

fn media_public_base_url(s3: Option<&S3Config>) -> String {
    env_optional("MEDIA_PUBLIC_BASE_URL").map_or_else(
        || s3.map_or_else(|| "/media".to_owned(), S3Config::direct_base_url),
        |base| base.trim_end_matches('/').to_owned(),
    )
}

fn env_flag(key: &str, default: bool) -> bool {
    env_optional(key).map_or(default, |value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// `localhost`, `127.0.0.1`, and `[::1]` are the same browser machine.
/// Browsers send the host as typed, so a CORS list with only 127.0.0.1
/// would 403 logins from <http://localhost:5173>.
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

fn dicebear_url_from_env() -> Option<String> {
    parse_dicebear_url(&env_or("DICEBEAR_URL", DICEBEAR_DEFAULT_URL))
}

fn parse_dicebear_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    {
        return None;
    }
    Some(value.to_owned())
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

#[cfg(test)]
mod tests {
    use super::{S3Config, StorageBackend};

    fn s3(path_style: bool) -> S3Config {
        S3Config {
            endpoint: "https://silo.example.net".into(),
            bucket: "openvk".into(),
            region: "auto".into(),
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            path_style,
        }
    }

    #[test]
    fn storage_backend_equality() {
        assert_eq!(StorageBackend::S3, StorageBackend::S3);
        assert_ne!(StorageBackend::S3, StorageBackend::Memory);
    }

    #[test]
    fn bucket_address_follows_the_addressing_style() {
        assert_eq!(
            s3(true).direct_base_url(),
            "https://silo.example.net/openvk"
        );
        assert_eq!(
            s3(false).direct_base_url(),
            "https://openvk.silo.example.net"
        );
    }

    #[test]
    fn dicebear_url_off_disables_generation() {
        assert_eq!(super::parse_dicebear_url("off"), None);
        assert_eq!(super::parse_dicebear_url("0"), None);
        assert_eq!(super::parse_dicebear_url("false"), None);
        assert_eq!(super::parse_dicebear_url("  "), None);
        assert_eq!(
            super::parse_dicebear_url(super::DICEBEAR_DEFAULT_URL).as_deref(),
            Some(super::DICEBEAR_DEFAULT_URL)
        );
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
