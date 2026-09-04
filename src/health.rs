use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub postgres: bool,
    pub redis: bool,
    pub version: &'static str,
}

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let (status, body) = health_body(&state).await;
    (status, Json(body))
}

pub async fn api_health(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let (status, body) = health_body(&state).await;
    crate::codec::protobuf_response(
        status,
        &crate::pb::Health {
            status: body.status.to_owned(),
            postgres: body.postgres,
            redis: body.redis,
            version: body.version.to_owned(),
        },
    )
}

async fn health_body(state: &AppState) -> (StatusCode, HealthResponse) {
    let postgres = state.db.ping().await.is_ok();
    let mut redis = state.redis.clone();
    let redis_ok = redis::cmd("PING")
        .query_async::<String>(&mut redis)
        .await
        .is_ok();

    let healthy = postgres && redis_ok;
    let body = HealthResponse {
        status: if healthy { "ok" } else { "degraded" },
        postgres,
        redis: redis_ok,
        version: crate::identity::BUILD_ID,
    };

    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, body)
}

pub async fn ready(State(state): State<AppState>) -> StatusCode {
    let postgres = state.db.ping().await.is_ok();
    let mut redis = state.redis.clone();
    let redis_ok = redis::cmd("PING")
        .query_async::<String>(&mut redis)
        .await
        .is_ok();

    if postgres && redis_ok {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
