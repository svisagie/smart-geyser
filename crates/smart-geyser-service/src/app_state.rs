//! `AppState` — the data shared between the scheduler and all API handlers.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use smart_geyser_core::models::{EngineConfig, GeyserState};
use smart_geyser_core::shared_state::SharedState;
use smart_geyser_core::system::HeatingSystem;

/// Immutable metadata about the configured providers.
#[derive(Debug, Clone)]
pub struct ProviderMeta {
    pub geyser_name: &'static str,
    pub system: HeatingSystem,
}

/// Mutable snapshot updated by the scheduler on every tick.
#[derive(Debug, Default)]
pub struct TickSnapshot {
    pub geyser: Option<GeyserState>,
    pub next_predicted_use: Option<DateTime<Utc>>,
    pub preheat_starts_at: Option<DateTime<Utc>>,
}

/// State shared between the axum handlers and the scheduler.
#[derive(Clone)]
pub struct AppState {
    /// Engine / boost / smart-stop flags.
    pub shared: SharedState,
    /// Latest geyser state + computed fields; updated each tick.
    pub snapshot: Arc<RwLock<TickSnapshot>>,
    /// Static provider metadata.
    pub provider: ProviderMeta,
    /// Current heating setpoint (°C), adjustable via API.
    pub setpoint_c: Arc<RwLock<f32>>,
    /// Engine configuration (setpoint excluded — read from `setpoint_c`).
    pub engine_config: EngineConfig,
    /// Tick interval in seconds.
    pub tick_interval_secs: u32,
}

impl AppState {
    #[must_use]
    pub fn new(
        shared: SharedState,
        provider: ProviderMeta,
        setpoint_c: Arc<RwLock<f32>>,
        engine_config: EngineConfig,
        tick_interval_secs: u32,
    ) -> Self {
        Self {
            shared,
            snapshot: Arc::new(RwLock::new(TickSnapshot::default())),
            provider,
            setpoint_c,
            engine_config,
            tick_interval_secs,
        }
    }
}
