//! Process identity stamped on every HTTP response.
//!
//! `x-openvk-build` is the short git commit (GitHub-style, e.g. `a229ef3`).
//! `x-openvk-instance` is the machine hostname so replicas on different hosts
//! can be told apart. Set `OPENVK_INSTANCE_ID` to pin it (pod name).

use axum::http::{HeaderName, HeaderValue};
use uuid::Uuid;

/// Compile-time git short hash (`a229ef3`), or `OPENVK_BUILD` when set.
pub const BUILD_ID: &str = env!("OPENVK_BUILD");

pub const BUILD_HEADER: HeaderName = HeaderName::from_static("x-openvk-build");
pub const INSTANCE_HEADER: HeaderName = HeaderName::from_static("x-openvk-instance");

#[derive(Clone, Debug)]
pub struct ProcessIdentity {
    pub build: HeaderValue,
    pub instance: HeaderValue,
}

impl Default for ProcessIdentity {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessIdentity {
    pub fn new() -> Self {
        let instance = resolve_instance_id();
        Self {
            build: HeaderValue::from_static(BUILD_ID),
            instance: HeaderValue::from_str(&instance)
                .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
        }
    }

    #[must_use]
    pub fn instance_id(&self) -> &str {
        self.instance.to_str().unwrap_or("unknown")
    }
}

fn resolve_instance_id() -> String {
    pinned_instance_id()
        .or_else(machine_hostname)
        .unwrap_or_else(|| Uuid::now_v7().to_string())
}

fn pinned_instance_id() -> Option<String> {
    std::env::var("OPENVK_INSTANCE_ID")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| is_instance_id(value))
}

fn machine_hostname() -> Option<String> {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| is_instance_id(name))
}

fn is_instance_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+'))
}

#[cfg(test)]
mod tests {
    use super::{BUILD_ID, is_instance_id};

    #[test]
    fn build_id_is_header_safe() {
        assert!(!BUILD_ID.is_empty());
        assert!(
            BUILD_ID
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | '_'))
        );
    }

    #[test]
    fn instance_id_rejects_spaces_and_empty() {
        assert!(is_instance_id("openvk-api-7f3c"));
        assert!(is_instance_id("0193a8c0-1234-7abc-8000-000000000001"));
        assert!(is_instance_id("MacBook-Pro.local"));
        assert!(!is_instance_id(""));
        assert!(!is_instance_id("bad id"));
        assert!(!is_instance_id(&"x".repeat(129)));
    }

    #[test]
    fn instance_id_defaults_to_hostname() {
        let resolved = super::resolve_instance_id();
        if let Some(pinned) = super::pinned_instance_id() {
            assert_eq!(resolved, pinned);
            return;
        }
        let host = super::machine_hostname().expect("machine hostname");
        assert_eq!(resolved, host);
    }
}
