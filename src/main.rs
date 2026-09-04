//! `OpenVK` REST and WebSocket API.

#![warn(clippy::all, clippy::pedantic)]

use openvk_backend::{AppError, AppState, config_from_env, serve};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("openvk_backend=debug,tower_http=debug,axum=info")
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config_from_env()?;
    let state = AppState::connect(config).await?;
    serve(state).await
}
