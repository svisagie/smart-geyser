//! GET + POST `/api/provider-config` — read and update the geyser provider settings.
//!
//! POST persists the new config to `<data_dir>/provider-config.json` (the
//! service overlay file) and then signals axum to shut down gracefully.
//! The HA Supervisor restarts the add-on, picking up the new provider on
//! the next start.  The engine settings section is preserved when writing.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::{GeyserProviderConfig, ServiceOverlay};

#[derive(Deserialize)]
pub struct SetProviderConfigBody {
    pub geyser: GeyserProviderConfig,
}

pub async fn get_provider_config(State(state): State<AppState>) -> Json<Value> {
    let path = state.data_dir.join("provider-config.json");
    if path.exists() {
        match ServiceOverlay::load(&path) {
            Ok(overlay) => {
                if let Some(ref geyser) = overlay.geyser {
                    if let Ok(geyser_val) = serde_json::to_value(geyser) {
                        return Json(json!({"configured": true, "geyser": geyser_val}));
                    }
                }
            }
            Err(e) => warn!("failed to read provider-config.json: {e:#}"),
        }
    }
    Json(json!({"configured": false}))
}

pub async fn post_provider_config(
    State(state): State<AppState>,
    Json(body): Json<SetProviderConfigBody>,
) -> (StatusCode, Json<Value>) {
    if state.data_dir.as_os_str().is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"ok": false, "error": "no data_dir configured"})),
        );
    }
    let path = state.data_dir.join("provider-config.json");
    // Load existing overlay so we preserve engine settings.
    let mut overlay = if path.exists() {
        ServiceOverlay::load(&path).unwrap_or_default()
    } else {
        ServiceOverlay::default()
    };
    overlay.geyser = Some(body.geyser);
    if let Err(e) = overlay.save(&path) {
        warn!("failed to write provider-config.json: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("{e}")})),
        );
    }
    info!("provider config saved — signalling graceful restart");
    state.trigger_shutdown();
    (StatusCode::OK, Json(json!({"ok": true, "restart_required": true})))
}
