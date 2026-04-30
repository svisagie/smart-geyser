use axum::extract::State;
use axum::response::Json;
use serde::Serialize;

use crate::app_state::AppState;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub setpoint_c: f32,
    pub hysteresis_c: f32,
    pub preheat_threshold: f32,
    pub late_use_threshold: f32,
    pub cutoff_buffer_min: u32,
    pub safety_margin_min: u32,
    pub legionella_interval_days: u32,
    pub decay_factor: f32,
    pub tick_interval_secs: u32,
}

pub async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    let setpoint_c = *state.setpoint_c.read().await;
    let cfg = &state.engine_config;
    Json(ConfigResponse {
        setpoint_c,
        hysteresis_c: cfg.hysteresis_c,
        preheat_threshold: cfg.preheat_threshold,
        late_use_threshold: cfg.late_use_threshold,
        cutoff_buffer_min: cfg.cutoff_buffer_min,
        safety_margin_min: cfg.safety_margin_min,
        legionella_interval_days: cfg.legionella_interval_days,
        decay_factor: cfg.decay_factor,
        tick_interval_secs: state.tick_interval_secs,
    })
}
