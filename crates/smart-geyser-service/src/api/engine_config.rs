//! GET + POST `/api/engine-config` — read and update engine / scheduler settings.
//!
//! GET returns the live engine config loaded at startup (from the overlay).
//! POST saves the new settings to the overlay, preserving provider config,
//! and signals a graceful restart so the scheduler re-initialises cleanly.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::app_state::AppState;
use crate::config::{EngineSettings, ServiceOverlay};

#[derive(Deserialize)]
pub struct SetEngineConfigBody {
    pub setpoint_c: f32,
    pub hysteresis_c: f32,
    pub preheat_threshold: f32,
    pub late_use_threshold: f32,
    pub cutoff_buffer_min: u32,
    pub safety_margin_min: u32,
    pub decay_factor: f32,
    pub legionella_interval_days: u32,
    pub tick_interval_secs: u32,
}

pub async fn get_engine_config(State(state): State<AppState>) -> Json<Value> {
    let setpoint_c = *state.setpoint_c.read().await;
    let cfg = &state.engine_config;
    Json(json!({
        "setpoint_c": setpoint_c,
        "hysteresis_c": cfg.hysteresis_c,
        "preheat_threshold": cfg.preheat_threshold,
        "late_use_threshold": cfg.late_use_threshold,
        "cutoff_buffer_min": cfg.cutoff_buffer_min,
        "safety_margin_min": cfg.safety_margin_min,
        "decay_factor": cfg.decay_factor,
        "legionella_interval_days": cfg.legionella_interval_days,
        "tick_interval_secs": state.tick_interval_secs,
    }))
}

pub async fn post_engine_config(
    State(state): State<AppState>,
    Json(body): Json<SetEngineConfigBody>,
) -> (StatusCode, Json<Value>) {
    if state.data_dir.as_os_str().is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"ok": false, "error": "no data_dir configured"})),
        );
    }
    let path = state.data_dir.join("provider-config.json");
    let mut overlay = if path.exists() {
        ServiceOverlay::load(&path).unwrap_or_default()
    } else {
        ServiceOverlay::default()
    };
    overlay.engine = EngineSettings {
        setpoint_c: body.setpoint_c,
        hysteresis_c: body.hysteresis_c,
        preheat_threshold: body.preheat_threshold,
        late_use_threshold: body.late_use_threshold,
        cutoff_buffer_min: body.cutoff_buffer_min,
        safety_margin_min: body.safety_margin_min,
        decay_factor: body.decay_factor,
        legionella_interval_days: body.legionella_interval_days,
        tick_interval_secs: body.tick_interval_secs,
    };
    if let Err(e) = overlay.save(&path) {
        warn!("failed to write provider-config.json: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("{e}")})),
        );
    }
    info!("engine config saved — signalling graceful restart");
    state.trigger_shutdown();
    (
        StatusCode::OK,
        Json(json!({"ok": true, "restart_required": true})),
    )
}
