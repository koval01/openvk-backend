use std::path::Path;

use uuid::Uuid;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Photo,
    Audio,
    Video,
    Avatar,
}

impl MediaKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Avatar => "avatar",
        }
    }

    #[must_use]
    pub const fn max_bytes(self) -> u64 {
        match self {
            Self::Photo | Self::Avatar => 10 * 1024 * 1024,
            Self::Audio => 20 * 1024 * 1024,
            Self::Video => 50 * 1024 * 1024,
        }
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn accepts_mime(self, mime: &str) -> bool {
        let mime = mime.to_ascii_lowercase();
        match self {
            Self::Photo | Self::Avatar => matches!(
                mime.as_str(),
                "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/webp"
            ),
            Self::Audio => matches!(
                mime.as_str(),
                "audio/mpeg"
                    | "audio/mp3"
                    | "audio/wav"
                    | "audio/x-wav"
                    | "audio/wave"
                    | "audio/ogg"
                    | "audio/webm"
                    | "audio/mp4"
                    | "audio/aac"
                    | "audio/flac"
                    | "audio/x-flac"
            ),
            Self::Video => matches!(
                mime.as_str(),
                "video/mp4" | "video/webm" | "video/quicktime"
            ),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn extension(self, mime: &str, filename: &str) -> Result<&'static str, AppError> {
        let from_name = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        if let Some(ext) = from_name.as_deref() {
            let allowed = match self {
                Self::Photo | Self::Avatar => {
                    matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp")
                }
                Self::Audio => {
                    matches!(ext, "mp3" | "wav" | "ogg" | "webm" | "m4a" | "aac" | "flac")
                }
                Self::Video => matches!(ext, "mp4" | "webm" | "mov"),
            };
            if allowed {
                return Ok(match ext {
                    "jpeg" => "jpg",
                    "m4a" => "m4a",
                    other => match other {
                        "jpg" => "jpg",
                        "png" => "png",
                        "gif" => "gif",
                        "webp" => "webp",
                        "mp3" => "mp3",
                        "wav" => "wav",
                        "ogg" => "ogg",
                        "webm" => "webm",
                        "aac" => "aac",
                        "flac" => "flac",
                        "mp4" => "mp4",
                        "mov" => "mov",
                        _ => "bin",
                    },
                });
            }
        }
        Ok(match mime.to_ascii_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
            "audio/ogg" => "ogg",
            "audio/webm" | "video/webm" => "webm",
            "audio/mp4" => "m4a",
            "audio/aac" => "aac",
            "audio/flac" | "audio/x-flac" => "flac",
            "video/mp4" => "mp4",
            "video/quicktime" => "mov",
            _ => {
                return Err(AppError::Validation(format!(
                    "unsupported file type for {}",
                    self.as_str()
                )));
            }
        })
    }

    pub fn validate_size(self, len: u64) -> Result<(), AppError> {
        if len == 0 {
            return Err(AppError::Validation("file is empty".into()));
        }
        if len > self.max_bytes() {
            return Err(AppError::Validation(format!(
                "{} is larger than {} bytes",
                self.as_str(),
                self.max_bytes()
            )));
        }
        Ok(())
    }
}

#[must_use]
pub fn infer_mime(filename: &str, declared: Option<&str>) -> String {
    if let Some(mime) = declared {
        let mime = mime.to_ascii_lowercase();
        if mime != "application/octet-stream" && !mime.is_empty() {
            return mime;
        }
    }
    match Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg".into(),
        Some("png") => "image/png".into(),
        Some("gif") => "image/gif".into(),
        Some("webp") => "image/webp".into(),
        Some("mp3") => "audio/mpeg".into(),
        Some("wav") => "audio/wav".into(),
        Some("ogg") => "audio/ogg".into(),
        Some("m4a") => "audio/mp4".into(),
        Some("aac") => "audio/aac".into(),
        Some("flac") => "audio/flac".into(),
        Some("mp4") => "video/mp4".into(),
        Some("webm") => {
            if filename.to_ascii_lowercase().contains("audio") {
                "audio/webm".into()
            } else {
                "video/webm".into()
            }
        }
        Some("mov") => "video/quicktime".into(),
        _ => declared
            .unwrap_or("application/octet-stream")
            .to_ascii_lowercase(),
    }
}

#[must_use]
pub fn storage_key(owner_user_id: i64, kind: MediaKind, ext: &str) -> String {
    format!("{owner_user_id}/{}/{}.{ext}", kind.as_str(), Uuid::now_v7())
}

#[must_use]
pub fn public_media_url(storage_key: &str) -> String {
    format!("/media/{storage_key}")
}

#[cfg(test)]
mod tests {
    use super::{MediaKind, infer_mime, public_media_url, storage_key};

    #[test]
    fn photo_mime_and_size_rules() {
        assert!(MediaKind::Photo.accepts_mime("image/png"));
        assert!(!MediaKind::Photo.accepts_mime("application/pdf"));
        assert!(MediaKind::Photo.validate_size(12).is_ok());
        assert!(MediaKind::Photo.validate_size(0).is_err());
        assert!(
            MediaKind::Photo
                .validate_size(MediaKind::Photo.max_bytes() + 1)
                .is_err()
        );
        assert_eq!(
            MediaKind::Photo.extension("image/png", "shot.PNG").unwrap(),
            "png"
        );
    }

    #[test]
    fn infers_mime_from_filename() {
        assert_eq!(
            infer_mime("track.mp3", Some("application/octet-stream")),
            "audio/mpeg"
        );
        assert_eq!(infer_mime("clip.webm", None), "video/webm");
    }

    #[test]
    fn storage_keys_are_scoped_to_owner_and_kind() {
        let key = storage_key(9, MediaKind::Audio, "mp3");
        assert!(key.starts_with("9/audio/"));
        assert!(key.contains(".mp3"));
        assert_eq!(public_media_url(&key), format!("/media/{key}"));
    }
}
