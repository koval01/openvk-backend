use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("database error")]
    Database(#[from] sea_orm::DbErr),
    #[error("redis error")]
    Redis(#[from] redis::RedisError),
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("rate limited")]
    RateLimited,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("unsupported media type")]
    UnsupportedMediaType,
    #[error("internal error")]
    Internal(String),
}

impl AppError {
    pub fn internal(message: impl Into<String>) -> Self {
        let message = message.into();
        tracing::error!(%message, "internal error");
        Self::Internal(message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, public_message) = match &self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                self.to_string(),
            ),
            Self::Validation(message) => {
                (StatusCode::BAD_REQUEST, "validation_error", message.clone())
            }
            Self::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "request must be application/x-protobuf".into(),
            ),
            Self::Config(_)
            | Self::Database(_)
            | Self::Redis(_)
            | Self::Io(_)
            | Self::Internal(_) => {
                tracing::error!(error = %self, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal error".to_owned(),
                )
            }
        };

        crate::codec::protobuf_response(
            status,
            &crate::pb::Error {
                error: error.to_owned(),
                message: public_message,
            },
        )
    }
}
