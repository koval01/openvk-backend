use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::storage::ObjectBody;

#[derive(Clone, Default)]
pub struct MemoryStore {
    objects: Arc<RwLock<HashMap<String, ObjectBody>>>,
}

impl MemoryStore {
    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), AppError> {
        self.objects.write().await.insert(
            key.to_owned(),
            ObjectBody {
                bytes,
                content_type: content_type.to_owned(),
            },
        );
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        self.objects
            .read()
            .await
            .get(key)
            .cloned()
            .ok_or(AppError::NotFound)
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.objects.write().await.remove(key);
        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        Ok(self.objects.read().await.contains_key(key))
    }
}
