mod disk;
mod memory;
mod r2;

use bytes::Bytes;

use crate::config::{Config, StorageBackend};
use crate::error::AppError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectBody {
    pub bytes: Bytes,
    pub content_type: String,
}

#[derive(Clone)]
pub enum Storage {
    Memory(memory::MemoryStore),
    Disk(disk::DiskStore),
    R2(r2::R2Store),
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
            StorageBackend::Disk => {
                Ok(Self::Disk(disk::DiskStore::new(config.media_root.clone())?))
            }
            StorageBackend::R2 => Ok(Self::R2(r2::R2Store::from_config(config)?)),
        }
    }

    #[must_use]
    pub const fn backend_name(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Disk(_) => "disk",
            Self::R2(_) => "r2",
        }
    }

    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), AppError> {
        match self {
            Self::Memory(store) => store.put(key, bytes, content_type).await,
            Self::Disk(store) => store.put(key, bytes, content_type).await,
            Self::R2(store) => store.put(key, bytes, content_type).await,
        }
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        match self {
            Self::Memory(store) => store.get(key).await,
            Self::Disk(store) => store.get(key).await,
            Self::R2(store) => store.get(key).await,
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match self {
            Self::Memory(store) => store.delete(key).await,
            Self::Disk(store) => store.delete(key).await,
            Self::R2(store) => store.delete(key).await,
        }
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        match self {
            Self::Memory(store) => store.exists(key).await,
            Self::Disk(store) => store.exists(key).await,
            Self::R2(store) => store.exists(key).await,
        }
    }

    pub async fn delete_keys(&self, keys: &[String]) -> Result<(), AppError> {
        for key in keys {
            self.delete(key).await?;
        }
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
    async fn disk_roundtrip_and_delete() {
        let root = std::env::temp_dir().join(format!(
            "openvk-disk-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let storage = Storage::Disk(super::disk::DiskStore::new(root.clone()).unwrap());
        storage
            .put("9/photo/a.png", Bytes::from_static(b"png"), "image/png")
            .await
            .unwrap();
        let got = storage.get("9/photo/a.png").await.unwrap();
        assert_eq!(got.bytes.as_ref(), b"png");
        assert!(storage.exists("9/photo/a.png").await.unwrap());
        storage.delete("9/photo/a.png").await.unwrap();
        assert!(!storage.exists("9/photo/a.png").await.unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_escape_keys() {
        assert!(validate_storage_key("../secret").is_err());
        assert!(validate_storage_key("/abs").is_err());
        assert!(validate_storage_key("a//b").is_err());
        assert!(validate_storage_key("9/photo/ok.png").is_ok());
    }
}
