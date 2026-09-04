//! `OpenVK` REST and WebSocket API.

#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

pub(crate) mod cache;
pub(crate) mod codec;
pub(crate) mod config;
pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod health;
pub(crate) mod http_security;
pub(crate) mod identity;
pub(crate) mod ids;
pub(crate) mod middleware;
pub(crate) mod modules;
pub(crate) mod password;
pub mod pb;
pub(crate) mod security;
pub(crate) mod state;
pub(crate) mod storage;
pub(crate) mod turnstile;
pub(crate) mod vault;

pub use crate::codec::{PROTOBUF_MIME, decode as decode_pb, encode as encode_pb};
pub use crate::config::{Config, StorageBackend};
pub use crate::error::AppError;
pub use crate::identity::{BUILD_HEADER, BUILD_ID, INSTANCE_HEADER};
pub use crate::ids::{PUBLIC_ID_MAX, PUBLIC_ID_MIN, is_public_id, wall_permalink};
pub use crate::security::{ChallengeResponse, seal_fields};
pub use crate::state::AppState;
pub use crate::storage::Storage;
pub use crate::turnstile::{
    DUMMY_FAIL_SECRET, DUMMY_PASS_SECRET, DUMMY_SPENT_SECRET, DUMMY_TOKEN, SITEVERIFY_URL,
    Turnstile,
};

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, Method, StatusCode, header};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

pub fn build_router(state: AppState) -> Router {
    let cors = cors_layer(&state.config.cors_origins);
    let body_limit = usize::try_from(state.config.max_upload_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(1_048_576);
    let http = modules::http_router()
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(120),
        ));

    Router::new()
        .merge(modules::ws_router())
        .merge(http)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::protect::protect,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::latency::latency,
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

fn cors_layer(origins: &[String]) -> CorsLayer {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ];
    let request_headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::COOKIE,
        header::RANGE,
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("x-csrf-token"),
        HeaderName::from_static("x-openvk-challenge"),
    ];
    let expose = [
        header::ACCEPT_RANGES,
        header::CONTENT_RANGE,
        header::CONTENT_LENGTH,
        crate::identity::BUILD_HEADER,
        crate::identity::INSTANCE_HEADER,
        HeaderName::from_static("x-request-id"),
    ];

    if origins.iter().any(|origin| origin == "*") {
        tracing::warn!(
            "CORS_ORIGIN=* disables credentialed CORS; set explicit origins in production"
        );
        return CorsLayer::new()
            .allow_origin(AllowOrigin::any())
            .allow_methods(methods)
            .allow_headers(request_headers)
            .expose_headers(expose)
            .max_age(Duration::from_secs(600));
    }

    let parsed = origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods(methods)
        .allow_headers(request_headers)
        .expose_headers(expose)
        .allow_credentials(true)
        .max_age(Duration::from_secs(600))
}

pub async fn serve(state: AppState) -> Result<(), AppError> {
    let listen_addr = state.config.listen_addr;
    tracing::info!(
        %listen_addr,
        build = crate::identity::BUILD_ID,
        instance = state.identity.instance_id(),
        "openvk-backend listening"
    );
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    tracing::info!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

pub fn config_from_env() -> Result<Config, AppError> {
    Config::from_env()
}
