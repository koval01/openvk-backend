use std::path::{Path, PathBuf};

use bytes::Bytes;
use tokio::fs;

use crate::error::AppError;
use crate::storage::{ObjectBody, validate_storage_key};

#[derive(Clone)]
pub struct DiskStore {
    root: PathBuf,
}

impl DiskStore {
    pub fn new(root: PathBuf) -> Result<Self, AppError> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root: root.canonicalize().unwrap_or(root),
        })
    }

    fn path_for(&self, key: &str) -> Result<PathBuf, AppError> {
        validate_storage_key(key)?;
        Ok(self.root.join(key))
    }

    pub async fn put(&self, key: &str, bytes: Bytes, _content_type: &str) -> Result<(), AppError> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension(format!(
            "{}.tmp",
            path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("bin")
        ));
        fs::write(&tmp, &bytes).await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        let path = self.path_for(key)?;
        let bytes = fs::read(&path).await.map_err(map_io)?;
        Ok(ObjectBody {
            bytes: Bytes::from(bytes),
            content_type: mime_from_key(key).to_owned(),
        })
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let path = self.path_for(key)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        Ok(fs::try_exists(self.path_for(key)?).await?)
    }
}

fn map_io(error: std::io::Error) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AppError::NotFound
    } else {
        error.into()
    }
}

fn mime_from_key(key: &str) -> &'static str {
    match Path::new(key)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    }
}
