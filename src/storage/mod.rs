mod memory;
mod s3;

use bytes::Bytes;
use futures_util::stream::{self, StreamExt, TryStreamExt};

use crate::config::{Config, StorageBackend};
use crate::error::AppError;

/// Deleting an album means deleting every photo in it; a little concurrency
/// keeps that from turning into a long serial round trip per object.
const DELETE_CONCURRENCY: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectBody {
    pub bytes: Bytes,
    pub content_type: String,
}

#[derive(Clone)]
pub enum Storage {
    Memory(memory::MemoryStore),
    S3(s3::S3Store),
}

pub fn validate_storage_key(key: &str) -> Result<(), AppError> {
    if key.is_empty()
        || key.starts_with('/')
        || key.contains('\0')
        || key
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
    {
        return Err(AppError::Validation("invalid storage key".into()));
    }
    Ok(())
}

impl Storage {
    #[must_use]
    pub fn memory() -> Self {
        Self::Memory(memory::MemoryStore::default())
    }

    pub fn from_config(config: &Config) -> Result<Self, AppError> {
        match config.storage_backend {
            StorageBackend::Memory => Ok(Self::memory()),
            StorageBackend::S3 => {
                let s3 = config.s3.clone().ok_or_else(|| {
                    AppError::Config("S3 storage is selected but not configured".into())
                })?;
                Ok(Self::S3(s3::S3Store::new(s3)?))
            }
        }
    }

    #[must_use]
    pub const fn backend_name(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::S3(_) => "s3",
        }
    }

    /// The bucket objects land in, for the startup log. Memory has none.
    #[must_use]
    pub fn bucket(&self) -> Option<&str> {
        match self {
            Self::Memory(_) => None,
            Self::S3(store) => Some(store.bucket()),
        }
    }

    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), AppError> {
        match self {
            Self::Memory(store) => store.put(key, bytes, content_type).await,
            Self::S3(store) => store.put(key, bytes, content_type).await,
        }
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        match self {
            Self::Memory(store) => store.get(key).await,
            Self::S3(store) => store.get(key).await,
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match self {
            Self::Memory(store) => store.delete(key).await,
            Self::S3(store) => store.delete(key).await,
        }
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        match self {
            Self::Memory(store) => store.exists(key).await,
            Self::S3(store) => store.exists(key).await,
        }
    }

    pub async fn delete_keys(&self, keys: &[String]) -> Result<(), AppError> {
        let mut deletions = Vec::with_capacity(keys.len());
        for key in keys {
            deletions.push(self.delete(key));
        }
        stream::iter(deletions)
            .buffer_unordered(DELETE_CONCURRENCY)
            .try_collect::<Vec<()>>()
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Storage, validate_storage_key};
    use bytes::Bytes;

    #[tokio::test]
    async fn memory_roundtrip_and_delete() {
        let storage = Storage::memory();
        storage
            .put("u/photo/a.png", Bytes::from_static(b"png"), "image/png")
            .await
            .unwrap();
        let got = storage.get("u/photo/a.png").await.unwrap();
        assert_eq!(got.bytes.as_ref(), b"png");
        assert_eq!(got.content_type, "image/png");
        assert!(storage.exists("u/photo/a.png").await.unwrap());
        storage.delete("u/photo/a.png").await.unwrap();
        assert!(!storage.exists("u/photo/a.png").await.unwrap());
        storage.delete("u/photo/a.png").await.unwrap();
    }

    #[tokio::test]
    async fn delete_keys_clears_every_object() {
        let storage = Storage::memory();
        let keys: Vec<String> = (0..20).map(|n| format!("9/photo/{n}.png")).collect();
        for key in &keys {
            storage
                .put(key, Bytes::from_static(b"png"), "image/png")
                .await
                .unwrap();
        }
        storage.delete_keys(&keys).await.unwrap();
        for key in &keys {
            assert!(!storage.exists(key).await.unwrap());
        }
    }

    #[test]
    fn rejects_path_escape_keys() {
        assert!(validate_storage_key("../secret").is_err());
        assert!(validate_storage_key("/abs").is_err());
        assert!(validate_storage_key("a//b").is_err());
        assert!(validate_storage_key("9/photo/ok.png").is_ok());
    }
}
