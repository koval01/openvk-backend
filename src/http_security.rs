//! Response security headers (CSP, framing, MIME sniffing).

use axum::http::{HeaderName, HeaderValue};

/// API responses are protobuf or JSON probes; lock down active content and embedding.
pub const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'";

pub fn security_header_pairs() -> [(&'static str, &'static str); 6] {
    [
        ("content-security-policy", API_CSP),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        ),
        ("cross-origin-opener-policy", "same-origin"),
    ]
}

pub fn insert_security_headers(headers: &mut axum::http::HeaderMap) {
    for (name, value) in security_header_pairs() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
}
