//! `AppState` — the data shared between the scheduler and all API handlers.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{watch, RwLock};

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
    /// Broadcast channel: serialised JSON of the latest status, sent each tick.
    /// SSE clients subscribe via `events_tx.subscribe()`.
    pub events_tx: tokio::sync::broadcast::Sender<String>,
    /// Most recent broadcast payload — sent immediately to new SSE connections.
    pub last_status_event: Arc<RwLock<Option<String>>>,
    /// Directory for persistent data files (`provider-config.json`, `pattern_store.json`).
    pub data_dir: PathBuf,
    /// Shutdown signal — POST /api/provider-config sends `true` to restart the service.
    shutdown_tx: Arc<watch::Sender<bool>>,
}

impl AppState {
    #[must_use]
    pub fn new(
        shared: SharedState,
        provider: ProviderMeta,
        setpoint_c: Arc<RwLock<f32>>,
        engine_config: EngineConfig,
        tick_interval_secs: u32,
        data_dir: PathBuf,
    ) -> Self {
        let (events_tx, _) = tokio::sync::broadcast::channel(32);
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            shared,
            snapshot: Arc::new(RwLock::new(TickSnapshot::default())),
            provider,
            setpoint_c,
            engine_config,
            tick_interval_secs,
            events_tx,
            last_status_event: Arc::new(RwLock::new(None)),
            data_dir,
            shutdown_tx: Arc::new(shutdown_tx),
        }
    }

    /// Subscribe to the shutdown signal; fires when `trigger_shutdown()` is called.
    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    /// Signal the process to restart (write config first, then call this).
    pub fn trigger_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}
