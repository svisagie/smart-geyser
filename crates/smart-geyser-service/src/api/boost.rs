use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::app_state::AppState;

#[derive(Deserialize)]
pub struct BoostRequest {
    pub duration_minutes: u32,
}

#[derive(Deserialize)]
pub struct SetpointRequest {
    pub temp_c: f32,
}

pub async fn post_boost(
    State(state): State<AppState>,
    Json(body): Json<BoostRequest>,
) -> impl IntoResponse {
    if body.duration_minutes < 1 || body.duration_minutes > 480 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "duration_minutes must be between 1 and 480"})),
        )
            .into_response();
    }

    let boost_until = Utc::now() + chrono::Duration::minutes(i64::from(body.duration_minutes));
    state.shared.set_boost_until(Some(boost_until)).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "boost_until": boost_until})),
    )
        .into_response()
}

pub async fn delete_boost(State(state): State<AppState>) -> impl IntoResponse {
    state.shared.set_boost_until(None).await;
    Json(json!({"ok": true}))
}

pub async fn post_setpoint(
    State(state): State<AppState>,
    Json(body): Json<SetpointRequest>,
) -> impl IntoResponse {
    if body.temp_c < 40.0 || body.temp_c > 75.0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "temp_c must be between 40.0 and 75.0"})),
        )
            .into_response();
    }

    *state.setpoint_c.write().await = body.temp_c;
    Json(json!({"ok": true})).into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum_test::TestServer;
    use serde_json::json;
    use tokio::sync::RwLock;

    use smart_geyser_core::shared_state::SharedState;
    use smart_geyser_core::system::HeatingSystem;

    use crate::app_state::{AppState, ProviderMeta};

    fn make_server() -> TestServer {
        let state = AppState::new(
            SharedState::new(),
            ProviderMeta {
                geyser_name: "T",
                system: HeatingSystem::ElectricOnly,
            },
            Arc::new(RwLock::new(60.0)),
            smart_geyser_core::models::EngineConfig::default(),
            60,
        );
        TestServer::new(super::super::router().with_state(state))
    }

    #[tokio::test]
    async fn post_boost_sets_shared_state() {
        let server = make_server();
        let resp = server
            .post("/api/boost")
            .json(&json!({"duration_minutes": 60}))
            .await;
        resp.assert_status_ok();
        let body = resp.json::<serde_json::Value>();
        assert_eq!(body["ok"], true);
        assert!(body["boost_until"].is_string());
    }

    #[tokio::test]
    async fn post_boost_out_of_range_returns_422() {
        let server = make_server();
        let resp = server
            .post("/api/boost")
            .json(&json!({"duration_minutes": 0}))
            .await;
        resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

        let resp2 = server
            .post("/api/boost")
            .json(&json!({"duration_minutes": 481}))
            .await;
        resp2.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn delete_boost_clears_state() {
        let server = make_server();
        server
            .post("/api/boost")
            .json(&json!({"duration_minutes": 30}))
            .await
            .assert_status_ok();

        let resp = server.delete("/api/boost").await;
        resp.assert_status_ok();
        assert_eq!(resp.json::<serde_json::Value>()["ok"], true);
    }

    #[tokio::test]
    async fn post_setpoint_valid() {
        let server = make_server();
        let resp = server
            .post("/api/setpoint")
            .json(&json!({"temp_c": 65.0}))
            .await;
        resp.assert_status_ok();
        assert_eq!(resp.json::<serde_json::Value>()["ok"], true);
    }

    #[tokio::test]
    async fn post_setpoint_out_of_range_returns_422() {
        let server = make_server();
        let resp = server
            .post("/api/setpoint")
            .json(&json!({"temp_c": 39.9}))
            .await;
        resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

        let resp2 = server
            .post("/api/setpoint")
            .json(&json!({"temp_c": 75.1}))
            .await;
        resp2.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }

    use axum::http::StatusCode;
}
