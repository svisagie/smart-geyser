use axum::response::Json;
use serde_json::{json, Value};

pub async fn get_opportunity_log() -> Json<Value> {
    Json(json!([]))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum_test::TestServer;
    use tokio::sync::RwLock;

    use smart_geyser_core::shared_state::SharedState;
    use smart_geyser_core::system::HeatingSystem;

    use crate::app_state::{AppState, ProviderMeta};

    #[tokio::test]
    async fn returns_empty_array() {
        let state = AppState::new(
            SharedState::new(),
            ProviderMeta {
                geyser_name: "T",
                system: HeatingSystem::ElectricOnly,
            },
            Arc::new(RwLock::new(60.0)),
            smart_geyser_core::models::EngineConfig::default(),
            60,
            std::path::PathBuf::new(),
            Arc::new(tokio::sync::Notify::new()),
        );
        let server = TestServer::new(super::super::router().with_state(state));
        let resp = server.get("/api/opportunity-log").await;
        resp.assert_status_ok();
        let body = resp.json::<serde_json::Value>();
        assert!(body.as_array().unwrap().is_empty());
    }
}
