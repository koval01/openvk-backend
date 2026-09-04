//! S3 API client: Cloudflare R2, Silo, `MinIO`.
//!
//! Requests are signed with AWS Signature Version 4 and never touch the filesystem.
//! Path-style addressing (`{endpoint}/{bucket}/{key}`) is the default because that is
//! what Silo and `MinIO` expect; R2 accepts either.

use std::fmt::Write as _;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Method, StatusCode};
use sha2::{Digest, Sha256};

use crate::config::S3Config;
use crate::error::AppError;
use crate::storage::{ObjectBody, validate_storage_key};

type HmacSha256 = Hmac<Sha256>;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const EMPTY_PAYLOAD_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Clone)]
pub struct S3Store {
    client: reqwest::Client,
    config: S3Config,
}

/// Everything a signed request needs, resolved from the key and the addressing style.
struct Target {
    url: String,
    host: String,
    canonical_uri: String,
}

impl S3Store {
    pub fn new(config: S3Config) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|err| AppError::Config(format!("could not build the S3 client: {err}")))?;
        Ok(Self { client, config })
    }

    pub fn bucket(&self) -> &str {
        &self.config.bucket
    }

    fn target(&self, key: &str) -> Result<Target, AppError> {
        validate_storage_key(key)?;
        let (scheme, authority) = split_endpoint(&self.config.endpoint)?;
        let encoded_key = uri_encode(key, false);
        let (host, path) = if self.config.path_style {
            (
                authority.to_owned(),
                format!("/{}/{encoded_key}", uri_encode(&self.config.bucket, true)),
            )
        } else {
            (
                format!("{}.{authority}", self.config.bucket),
                format!("/{encoded_key}"),
            )
        };
        Ok(Target {
            url: format!("{scheme}://{host}{path}"),
            host: host_header(scheme, &host),
            canonical_uri: path,
        })
    }

    async fn send(
        &self,
        method: &Method,
        key: &str,
        body: Option<(Bytes, &str)>,
    ) -> Result<reqwest::Response, AppError> {
        let target = self.target(key)?;
        let payload_hash = body.as_ref().map_or_else(
            || EMPTY_PAYLOAD_SHA256.to_owned(),
            |(bytes, _)| hex(&Sha256::digest(bytes)),
        );
        let content_type = body.as_ref().map(|(_, mime)| (*mime).to_owned());
        let now = Utc::now();
        let authorization =
            self.authorization(method, &target, content_type.as_deref(), &payload_hash, now);

        let mut request = self
            .client
            .request(method.clone(), &target.url)
            .header("x-amz-content-sha256", &payload_hash)
            .header("x-amz-date", amz_date(now))
            .header(reqwest::header::AUTHORIZATION, authorization);
        if let Some((bytes, mime)) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, mime)
                .body(bytes);
        }

        request.send().await.map_err(|err| {
            AppError::internal(format!(
                "s3 {method} failed for {}/{key}: {err}",
                self.config.bucket
            ))
        })
    }

    fn authorization(
        &self,
        method: &Method,
        target: &Target,
        content_type: Option<&str>,
        payload_hash: &str,
        now: DateTime<Utc>,
    ) -> String {
        let amz_date = amz_date(now);
        let date_stamp = now.format("%Y%m%d").to_string();

        let mut canonical_headers = String::new();
        let mut signed_headers = String::new();
        if let Some(content_type) = content_type {
            let _ = writeln!(canonical_headers, "content-type:{content_type}");
            signed_headers.push_str("content-type;");
        }
        let _ = writeln!(canonical_headers, "host:{}", target.host);
        let _ = writeln!(canonical_headers, "x-amz-content-sha256:{payload_hash}");
        let _ = writeln!(canonical_headers, "x-amz-date:{amz_date}");
        signed_headers.push_str("host;x-amz-content-sha256;x-amz-date");

        let canonical_request = format!(
            "{method}\n{}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
            target.canonical_uri
        );
        let scope = format!("{date_stamp}/{}/s3/aws4_request", self.config.region);
        let string_to_sign = format!(
            "{ALGORITHM}\n{amz_date}\n{scope}\n{}",
            hex(&Sha256::digest(canonical_request.as_bytes()))
        );

        let signature = hex(&sign(
            &self.signing_key(&date_stamp),
            string_to_sign.as_bytes(),
        ));
        format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.config.access_key_id
        )
    }

    fn signing_key(&self, date_stamp: &str) -> Vec<u8> {
        let start = format!("AWS4{}", self.config.secret_access_key);
        let date = sign(start.as_bytes(), date_stamp.as_bytes());
        let region = sign(&date, self.config.region.as_bytes());
        let service = sign(&region, b"s3");
        sign(&service, b"aws4_request")
    }

    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), AppError> {
        let response = self
            .send(&Method::PUT, key, Some((bytes, content_type)))
            .await?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(self.rejected("put", key, response).await)
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        let response = self.send(&Method::GET, key, None).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::NotFound);
        }
        if !response.status().is_success() {
            return Err(self.rejected("get", key, response).await);
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| AppError::internal(format!("s3 get body failed for {key}: {err}")))?;
        Ok(ObjectBody {
            bytes,
            content_type,
        })
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let response = self.send(&Method::DELETE, key, None).await?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(self.rejected("delete", key, response).await)
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let response = self.send(&Method::HEAD, key, None).await?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(false),
            status if status.is_success() => Ok(true),
            _ => Err(self.rejected("head", key, response).await),
        }
    }

    async fn rejected(&self, action: &str, key: &str, response: reqwest::Response) -> AppError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        AppError::internal(format!(
            "s3 {action} rejected for {}/{key} ({status}): {body}",
            self.config.bucket
        ))
    }
}

fn amz_date(now: DateTime<Utc>) -> String {
    now.format("%Y%m%dT%H%M%SZ").to_string()
}

fn sign(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// RFC 3986 encoding as S3 expects it in the canonical request.
fn uri_encode(value: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            b'/' if !encode_slash => out.push('/'),
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

fn split_endpoint(endpoint: &str) -> Result<(&str, &str), AppError> {
    endpoint
        .split_once("://")
        .filter(|(scheme, authority)| matches!(*scheme, "http" | "https") && !authority.is_empty())
        .ok_or_else(|| AppError::Config(format!("invalid S3 endpoint '{endpoint}'")))
}

/// The signature must match the Host header the client actually sends,
/// and clients drop the port when it is the default for the scheme.
fn host_header(scheme: &str, authority: &str) -> String {
    let default_port = if scheme == "https" { ":443" } else { ":80" };
    authority
        .strip_suffix(default_port)
        .unwrap_or(authority)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{S3Store, host_header, split_endpoint, uri_encode};
    use crate::config::S3Config;
    use crate::error::AppError;
    use chrono::TimeZone;
    use reqwest::Method;

    fn store(endpoint: &str, path_style: bool) -> S3Store {
        S3Store::new(S3Config {
            endpoint: endpoint.to_owned(),
            bucket: "openvk".into(),
            region: "auto".into(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            path_style,
        })
        .expect("client")
    }

    #[test]
    fn path_style_addresses_the_bucket_in_the_path() {
        let target = store("http://silo:9000", true)
            .target("9/photo/a.png")
            .unwrap();
        assert_eq!(target.url, "http://silo:9000/openvk/9/photo/a.png");
        assert_eq!(target.host, "silo:9000");
        assert_eq!(target.canonical_uri, "/openvk/9/photo/a.png");
    }

    #[test]
    fn virtual_style_addresses_the_bucket_in_the_host() {
        let target = store("https://acct.r2.cloudflarestorage.com", false)
            .target("9/photo/a.png")
            .unwrap();
        assert_eq!(
            target.url,
            "https://openvk.acct.r2.cloudflarestorage.com/9/photo/a.png"
        );
        assert_eq!(target.host, "openvk.acct.r2.cloudflarestorage.com");
    }

    #[test]
    fn rejects_unsafe_keys() {
        assert!(matches!(
            store("http://silo:9000", true).target("../etc/passwd"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn signature_covers_the_key_and_the_clock() {
        let store = store("http://silo:9000", true);
        let target = store.target("9/photo/a.png").unwrap();
        let other = store.target("9/photo/b.png").unwrap();
        let noon = chrono::Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let later = chrono::Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 1).unwrap();

        let first = store.authorization(&Method::GET, &target, None, "hash", noon);
        assert_eq!(
            first,
            store.authorization(&Method::GET, &target, None, "hash", noon),
            "the same request at the same second must sign identically"
        );
        assert_ne!(
            first,
            store.authorization(&Method::GET, &other, None, "hash", noon)
        );
        assert_ne!(
            first,
            store.authorization(&Method::GET, &target, None, "hash", later)
        );
        assert!(first.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert!(first.contains("Credential=AKIAIOSFODNN7EXAMPLE/20260904/auto/s3/aws4_request"));
    }

    #[test]
    fn a_body_adds_content_type_to_the_signed_headers() {
        let store = store("http://silo:9000", true);
        let target = store.target("9/photo/a.png").unwrap();
        let noon = chrono::Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();
        let signed = store.authorization(&Method::PUT, &target, Some("image/png"), "hash", noon);
        assert!(signed.contains("SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date"));
    }

    #[test]
    fn encodes_path_segments_but_keeps_separators() {
        assert_eq!(uri_encode("9/photo/a b.png", false), "9/photo/a%20b.png");
        assert_eq!(uri_encode("open vk", true), "open%20vk");
        assert_eq!(uri_encode("a-b_c.d~e", false), "a-b_c.d~e");
    }

    #[test]
    fn host_header_drops_the_default_port() {
        assert_eq!(
            host_header("https", "silo.example.net:443"),
            "silo.example.net"
        );
        assert_eq!(host_header("http", "silo:9000"), "silo:9000");
    }

    #[test]
    fn endpoint_must_carry_a_scheme() {
        assert!(split_endpoint("silo:9000").is_err());
        assert_eq!(
            split_endpoint("http://silo:9000").unwrap(),
            ("http", "silo:9000")
        );
    }
}
