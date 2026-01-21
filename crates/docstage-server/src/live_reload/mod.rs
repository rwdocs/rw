//! Live reload system for development mode.
//!
//! Provides file watching and WebSocket-based reload notifications
//! to connected clients when source files change.

pub mod manager;
mod websocket;

pub use manager::{LiveReloadManager, ReloadEvent};
pub use websocket::ws_handler;
