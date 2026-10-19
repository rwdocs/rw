//! WebSocket handler for live reload.
//!
//! Handles WebSocket connections and forwards reload events to clients.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use super::manager::{LiveReloadManager, ReloadEvent};

/// Handle WebSocket upgrade for live reload.
pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(live_reload): State<Arc<LiveReloadManager>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, live_reload))
}

/// Handle an established WebSocket connection.
async fn handle_socket(mut socket: WebSocket, live_reload: Arc<LiveReloadManager>) {
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
