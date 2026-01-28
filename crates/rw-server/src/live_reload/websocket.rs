//! WebSocket handler for live reload.
//!
//! Handles WebSocket connections and forwards reload events to clients.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use super::manager::ReloadEvent;
use crate::state::AppState;

/// Handle WebSocket upgrade for live reload.
pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle an established WebSocket connection.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let Some(ref live_reload) = state.live_reload else {
        // Live reload not enabled, close connection
        return;
    };

    let mut receiver: broadcast::Receiver<ReloadEvent> = live_reload.subscribe();

    loop {
        tokio::select! {
            // Forward reload events to client
            result = receiver.recv() => {
                match result {
                    Ok(event) => {
                        let msg = serde_json::to_string(&event).unwrap();
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
            // Handle client messages (for keepalive)
            result = socket.recv() => {
                match result {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}
