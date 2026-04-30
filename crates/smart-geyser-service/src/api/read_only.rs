use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use tracing::info;

use crate::app_state::AppState;

pub async fn enable(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    state.shared.set_read_only(true).await;
    info!("read-only mode ENABLED — element control suspended");
    (StatusCode::OK, Json(json!({"read_only_mode": true})))
}

pub async fn disable(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    state.shared.set_read_only(false).await;
    info!("read-only mode DISABLED — element control resumed");
    (StatusCode::OK, Json(json!({"read_only_mode": false})))
}
