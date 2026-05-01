mod api;
mod app_state;
mod config;
mod scheduler;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::{Notify, RwLock};
use tracing::{info, warn};

use smart_geyser_core::decision_engine::DecisionEngine;
use smart_geyser_core::event_detector::{EventDetector, EventDetectorConfig};
use smart_geyser_core::models::EngineConfig;
use smart_geyser_core::pattern_store::PatternStore;
use smart_geyser_core::provider::{GeyserCapability, GeyserProvider};
use smart_geyser_core::shared_state::SharedState;
use smart_geyser_core::system::HeatingSystem;
use smart_geyser_providers::geyserwala::GeyserwalaProvider;
use smart_geyser_providers::geyserwala_mqtt::GeyserwalaMqttProvider;

use app_state::{AppState, ProviderMeta};
use config::{GeyserProviderConfig, ServiceConfig, ServiceOverlay};
use scheduler::Scheduler;

fn parse_config_path() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    loop {
        match args.next().as_deref() {
            None => break,
            Some("--help" | "-h") => {
                println!("Usage: smart-geyser-service [--config <path>]");
                println!("  --config <path>  Path to config TOML (default: config.toml)");
                std::process::exit(0);
            }
            Some("--config") => {
                if let Some(path) = args.next() {
                    return Some(PathBuf::from(path));
                }
            }
            _ => {}
        }
    }
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "smart_geyser_service=info,warn".into()),
        )
        .init();

    let config_path = parse_config_path().unwrap_or_else(|| PathBuf::from("config.toml"));
    let cfg = ServiceConfig::load(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    let overlay = load_overlay(&cfg.data_dir);
    log_startup(&cfg, &config_path, &overlay);

    let tick_notify = Arc::new(Notify::new());
    let shared = SharedState::new();

    let (app_state, maybe_scheduler) = if let Some(geyser_config) = overlay.geyser.clone() {
        let (state, sched) =
            setup_configured(&cfg.data_dir, &overlay, geyser_config, shared, Arc::clone(&tick_notify)).await?;
        (state, Some(sched))
    } else {
        warn!("no provider configured — running in unconfigured mode");
        warn!("use the HA options flow or POST /api/provider-config to configure a provider");
        let sp = Arc::new(RwLock::new(overlay.engine.setpoint_c));
        let mut state = AppState::new(
            shared,
            ProviderMeta { geyser_name: "unconfigured", system: HeatingSystem::ElectricOnly },
            sp,
            overlay.engine.to_engine_config(HeatingSystem::ElectricOnly),
            overlay.engine.tick_interval_secs,
            cfg.data_dir.clone(),
            tick_notify,
        );
        state.configured = false;
        (state, None)
    };

    if let Some(scheduler) = maybe_scheduler {
        let interval = Duration::from_secs(u64::from(overlay.engine.tick_interval_secs));
        tokio::spawn(async move { scheduler.run(interval).await; });
    }

    let mut shutdown_rx = app_state.subscribe_shutdown();
    let router = api::router()
        .with_state(app_state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    info!(addr = %cfg.listen_addr, "starting smart-geyser-service");
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                () = shutdown_signal() => {}
                _ = shutdown_rx.changed() => {
                    info!("config updated — shutting down for restart");
                }
            }
        })
        .await?;

    Ok(())
}

async fn setup_configured(
    data_dir: &Path,
    overlay: &ServiceOverlay,
    geyser_config: GeyserProviderConfig,
    shared: SharedState,
    tick_notify: Arc<Notify>,
) -> anyhow::Result<(AppState, Scheduler)> {
    let geyser = build_provider(geyser_config, Arc::clone(&tick_notify)).await?;
    let engine_config = overlay.engine.to_engine_config(geyser.system());
    let initial_setpoint = read_initial_setpoint(geyser.as_ref(), &engine_config).await;
    let meta = ProviderMeta { geyser_name: geyser.name(), system: geyser.system() };
    let pattern_store = load_pattern_store(data_dir, engine_config.decay_factor);
    let engine = DecisionEngine::new(engine_config.clone(), pattern_store, shared.clone());
    let detector = EventDetector::new(EventDetectorConfig::default());
    let setpoint_arc = Arc::new(RwLock::new(initial_setpoint));
    let app_state = AppState::new(
        shared,
        meta,
        Arc::clone(&setpoint_arc),
        engine_config,
        overlay.engine.tick_interval_secs,
        data_dir.to_path_buf(),
        tick_notify,
    );
    let scheduler = Scheduler {
        geyser,
        engine,
        detector,
        app_state: app_state.clone(),
        data_dir: data_dir.to_path_buf(),
        setpoint_c: setpoint_arc,
    };
    Ok((app_state, scheduler))
}

async fn build_provider(
    config: GeyserProviderConfig,
    tick_notify: Arc<Notify>,
) -> anyhow::Result<Box<dyn GeyserProvider>> {
    match config {
        GeyserProviderConfig::Geyserwala(g) => Ok(Box::new(GeyserwalaProvider::new(g.into())?)),
        GeyserProviderConfig::GeyserwalaaMqtt(g) => {
            Ok(Box::new(GeyserwalaMqttProvider::new(g.into(), tick_notify).await?))
        }
    }
}

async fn read_initial_setpoint(geyser: &dyn GeyserProvider, cfg: &EngineConfig) -> f32 {
    if !geyser.capabilities().contains(&GeyserCapability::SetpointControl) {
        return cfg.setpoint_c;
    }
    match geyser.get_setpoint().await {
        Ok(Some(sp)) => {
            info!(device_setpoint_c = sp, config_setpoint_c = cfg.setpoint_c, "device setpoint overrides config");
            sp
        }
        Ok(None) => cfg.setpoint_c,
        Err(e) => {
            warn!("could not read setpoint from device: {e:#} — using config value");
            cfg.setpoint_c
        }
    }
}

fn load_pattern_store(data_dir: &Path, decay_factor: f32) -> PatternStore {
    let path = data_dir.join("pattern_store.json");
    if !data_dir.as_os_str().is_empty() && path.exists() {
        PatternStore::load_from_path(&path).unwrap_or_else(|_| PatternStore::new(decay_factor))
    } else {
        PatternStore::new(decay_factor)
    }
}

pub fn load_overlay(data_dir: &Path) -> ServiceOverlay {
    let path = data_dir.join("provider-config.json");
    if !data_dir.as_os_str().is_empty() && path.exists() {
        match ServiceOverlay::load(&path) {
            Ok(overlay) => {
                info!("loaded service overlay from {}", path.display());
                return overlay;
            }
            Err(e) => warn!("ignoring invalid service overlay: {e:#}"),
        }
    }
    ServiceOverlay::default()
}

fn log_startup(cfg: &ServiceConfig, config_path: &Path, overlay: &ServiceOverlay) {
    info!("=== Smart Geyser Controller v{} ===", env!("CARGO_PKG_VERSION"));
    info!(path = %config_path.display(), "config loaded");
    info!(addr = %cfg.listen_addr, data_dir = %cfg.data_dir.display(), "service");
    info!(
        tick_secs = overlay.engine.tick_interval_secs,
        setpoint_c = overlay.engine.setpoint_c,
        hysteresis_c = overlay.engine.hysteresis_c,
        preheat_threshold = overlay.engine.preheat_threshold,
        late_use_threshold = overlay.engine.late_use_threshold,
        cutoff_buffer_min = overlay.engine.cutoff_buffer_min,
        safety_margin_min = overlay.engine.safety_margin_min,
        legionella_interval_days = overlay.engine.legionella_interval_days,
        decay_factor = overlay.engine.decay_factor,
        "engine config"
    );
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    }
}
