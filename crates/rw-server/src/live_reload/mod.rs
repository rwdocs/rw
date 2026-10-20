//! Live reload system for development mode.
//!
//! Provides file watching and WebSocket-based reload notifications
//! to connected clients when source files change.

mod manager;
mod websocket;

pub(crate) use manager::LiveReloadManager;
#[cfg(test)]
pub(crate) use manager::{COMMENT_POLL_INTERVAL, ReloadEvent};
pub(crate) use websocket::ws_handler;
