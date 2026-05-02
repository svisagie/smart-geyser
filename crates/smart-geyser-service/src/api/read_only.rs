use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::ServiceOverlay;

pub async fn enable(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    state.shared.set_read_only(true).await;
    state.notify_tick();
    persist_read_only_mode(&state, true);
    info!("read-only mode ENABLED — element control suspended");
    (StatusCode::OK, Json(json!({"read_only_mode": true})))
}

pub async fn disable(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    state.shared.set_read_only(false).await;
    state.notify_tick();
    persist_read_only_mode(&state, false);
    info!("read-only mode DISABLED — element control resumed");
    (StatusCode::OK, Json(json!({"read_only_mode": false})))
}

fn persist_read_only_mode(state: &AppState, value: bool) {
    if state.data_dir.as_os_str().is_empty() {
        return;
    }
    let path = state.data_dir.join("provider-config.json");
    let mut overlay = if path.exists() {
        ServiceOverlay::load(&path).unwrap_or_default()
    } else {
        ServiceOverlay::default()
    };
    overlay.read_only_mode = value;
    if let Err(e) = overlay.save(&path) {
        warn!("failed to persist read-only mode: {e:#}");
    }
}
