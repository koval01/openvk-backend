use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use serde::Deserialize;

use crate::codec::{self, Proto};
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::modules::messenger::services;
use crate::pb;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub peer_id: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Proto<pb::MessageList>, AppError> {
    let Some(peer_id) = query.peer_id else {
        return Ok(Proto(codec::messages_to_pb(
            services::list_inbox(&state, auth.user_id).await?,
        )));
    };
    Ok(Proto(codec::messages_to_pb(
        services::list_thread(&state, auth.user_id, peer_id).await?,
    )))
}

pub async fn send(
    State(state): State<AppState>,
    auth: AuthUser,
    Proto(body): Proto<pb::SendMessage>,
) -> Result<Proto<pb::Message>, AppError> {
    Ok(Proto(codec::message_to_pb(
        &services::send(&state, auth.user_id, body.peer_id, body.text).await?,
    )))
}

pub async fn upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = state.events.subscribe();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));

    let hello = encode_socket_event("hello", "");
    if sender.send(WsMessage::Binary(hello.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sender.send(WsMessage::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(payload) => {
                        let frame = encode_socket_event("event", &payload);
                        if sender.send(WsMessage::Binary(frame.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        let incoming = pb::SocketEvent::decode(bytes.as_ref()).unwrap_or_default();
                        let echo = encode_socket_event("echo", &incoming.payload);
                        if sender.send(WsMessage::Binary(echo.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        if sender.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

fn encode_socket_event(kind: &str, payload: &str) -> Vec<u8> {
    pb::SocketEvent {
        r#type: kind.to_owned(),
        ts: chrono::Utc::now().timestamp(),
        payload: payload.to_owned(),
    }
    .encode_to_vec()
}
