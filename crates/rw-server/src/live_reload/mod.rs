//! Live reload system for development mode.
//!
//! Provides file watching and WebSocket-based reload notifications
//! to connected clients when source files change.

mod manager;
mod websocket;

pub(crate) use manager::{LiveReloadManager, ReloadEvent};
pub(crate) use websocket::ws_handler;
