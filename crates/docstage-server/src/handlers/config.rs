//! Configuration API endpoint.
//!
//! Returns client-side configuration for the frontend.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::state::AppState;

/// Response for GET /api/config.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigResponse {
    /// Whether live reload is enabled.
    live_reload_enabled: bool,
}

/// Handle GET /api/config.
pub(crate) async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        live_reload_enabled: state.live_reload_enabled(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_response_serialization() {
        let response = ConfigResponse {
            live_reload_enabled: true,
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["liveReloadEnabled"], true);
    }
}
