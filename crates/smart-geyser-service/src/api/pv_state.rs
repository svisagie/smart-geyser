use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};

pub async fn get_pv_state() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "no_pv_provider"})),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum_test::TestServer;
    use tokio::sync::RwLock;

    use smart_geyser_core::shared_state::SharedState;
    use smart_geyser_core::system::HeatingSystem;

    use crate::app_state::{AppState, ProviderMeta};

    fn make_app() -> TestServer {
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
    async fn no_pv_provider_returns_404() {
        let server = make_app();
        let resp = server.get("/api/pv-state").await;
        resp.assert_status(axum::http::StatusCode::NOT_FOUND);
        let body = resp.json::<serde_json::Value>();
        assert_eq!(body["error"], "no_pv_provider");
    }
}
