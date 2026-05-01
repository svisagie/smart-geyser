pub mod boost;
pub mod config;
pub mod events;
pub mod opportunity;
pub mod pv_state;
pub mod read_only;
pub mod status;

use axum::routing::{get, post};
use axum::Router;

use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(status::get_status))
        .route("/api/config", get(config::get_config))
        .route("/api/pv-state", get(pv_state::get_pv_state))
        .route(
            "/api/opportunity-log",
            get(opportunity::get_opportunity_log),
        )
        .route(
            "/api/boost",
            post(boost::post_boost).delete(boost::delete_boost),
        )
        .route("/api/setpoint", post(boost::post_setpoint))
        .route("/api/events", get(events::get_events))
        .route(
            "/api/read-only",
            post(read_only::enable).delete(read_only::disable),
        )
}
