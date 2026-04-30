use axum::extract::State;
use axum::response::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;

use smart_geyser_core::system::HeatingSystem;

use crate::app_state::AppState;

#[derive(Serialize)]
pub struct StatusResponse {
    pub system_type: &'static str,
    pub provider: &'static str,
    pub tank_temp_c: Option<f32>,
    pub collector_temp_c: Option<f32>,
    pub pump_active: Option<bool>,
    pub heating_active: Option<bool>,
    pub smart_stop_active: bool,
    pub preheat_active: bool,
    pub boost_until: Option<DateTime<Utc>>,
    pub next_predicted_use: Option<DateTime<Utc>>,
    pub preheat_starts_at: Option<DateTime<Utc>>,
    pub events_today: u32,
}

pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let snap = state.snapshot.read().await;
    let shared = state.shared.read().await;

    let system_type = match state.provider.system {
        HeatingSystem::ElectricOnly => "electric_only",
        HeatingSystem::SolarPumped => "solar_pumped",
        HeatingSystem::HeatPump { .. } => "heat_pump",
    };

    let (tank_temp_c, collector_temp_c, pump_active, heating_active) =
        if let Some(ref gs) = snap.geyser {
            (
                Some(gs.tank_temp_c),
                gs.collector_temp_c,
                gs.pump_active,
                Some(gs.heating_active),
            )
        } else {
            (None, None, None, None)
        };

    Json(StatusResponse {
        system_type,
        provider: state.provider.geyser_name,
        tank_temp_c,
        collector_temp_c,
        pump_active,
        heating_active,
        smart_stop_active: shared.smart_stop_active,
        preheat_active: shared.preheat_active,
        boost_until: shared.boost_until,
        next_predicted_use: snap.next_predicted_use,
        preheat_starts_at: snap.preheat_starts_at,
        events_today: 0,
    })
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
        let provider = ProviderMeta {
            geyser_name: "Test Provider",
            system: HeatingSystem::SolarPumped,
        };
        let state = AppState::new(SharedState::new(), provider, Arc::new(RwLock::new(60.0)));
        let router = super::super::router().with_state(state);
        TestServer::new(router)
    }

    #[tokio::test]
    async fn get_status_before_first_tick_returns_nulls() {
        let server = make_app();
        let resp = server.get("/api/status").await;
        resp.assert_status_ok();
        let body = resp.json::<serde_json::Value>();
        assert_eq!(body["system_type"], "solar_pumped");
        assert_eq!(body["provider"], "Test Provider");
        assert!(body["tank_temp_c"].is_null());
        assert_eq!(body["smart_stop_active"], false);
        assert_eq!(body["events_today"], 0);
    }
}
