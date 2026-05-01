mod api;
mod app_state;
mod config;
mod scheduler;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{info, warn};

use smart_geyser_core::decision_engine::DecisionEngine;
use smart_geyser_core::event_detector::{EventDetector, EventDetectorConfig};
use smart_geyser_core::pattern_store::PatternStore;
use smart_geyser_core::provider::{GeyserCapability, GeyserProvider};
use smart_geyser_core::shared_state::SharedState;
use smart_geyser_providers::geyserwala::GeyserwalaProvider;
use smart_geyser_providers::geyserwala_mqtt::GeyserwalaMqttProvider;

use app_state::{AppState, ProviderMeta};
use config::{GeyserProviderConfig, ServiceConfig};
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

    log_startup_config(&cfg, &config_path);

    // Build geyser provider.
    let geyser: Box<dyn GeyserProvider> = match cfg.geyser {
        GeyserProviderConfig::Geyserwala(g) => Box::new(GeyserwalaProvider::new(g.into())?),
        GeyserProviderConfig::GeyserwalaaMqtt(g) => {
            Box::new(GeyserwalaMqttProvider::new(g.into()).await?)
        }
    };

    let provider_meta = ProviderMeta {
        geyser_name: geyser.name(),
        system: geyser.system(),
    };

    // Load or create pattern store.
    let pattern_store = {
        let path = cfg.data_dir.join("pattern_store.json");
        if !cfg.data_dir.as_os_str().is_empty() && path.exists() {
            PatternStore::load_from_path(&path)
                .unwrap_or_else(|_| PatternStore::new(cfg.engine.decay_factor))
        } else {
            PatternStore::new(cfg.engine.decay_factor)
        }
    };

    // Align engine system type with the physical provider.
    let mut engine_config = cfg.engine.clone();
    engine_config.system = geyser.system();

    // If the provider exposes its setpoint, read it now — it is authoritative
    // over the config file so there is only one place to configure it.
    let initial_setpoint = if geyser
        .capabilities()
        .contains(&GeyserCapability::SetpointControl)
    {
        match geyser.get_setpoint().await {
            Ok(Some(sp)) => {
                info!(
                    device_setpoint_c = sp,
                    config_setpoint_c = engine_config.setpoint_c,
                    "device setpoint overrides config"
                );
                sp
            }
            Ok(None) => engine_config.setpoint_c,
            Err(e) => {
                warn!("could not read setpoint from device: {e:#} — using config value");
                engine_config.setpoint_c
            }
        }
    } else {
        engine_config.setpoint_c
    };

    // Shared state and setpoint Arc (scheduler and API share the same instance).
    let shared = SharedState::new();
    let setpoint_arc = Arc::new(RwLock::new(initial_setpoint));
    let app_state = AppState::new(
        shared.clone(),
        provider_meta,
        Arc::clone(&setpoint_arc),
        engine_config.clone(),
        cfg.tick_interval_secs,
    );

    let engine = DecisionEngine::new(engine_config, pattern_store, shared);
    let detector = EventDetector::new(EventDetectorConfig::default());

    let scheduler = Scheduler {
        geyser,
        engine,
        detector,
        app_state: app_state.clone(),
        data_dir: cfg.data_dir,
        setpoint_c: setpoint_arc,
    };

    let tick_interval = Duration::from_secs(u64::from(cfg.tick_interval_secs));

    tokio::spawn(async move {
        scheduler.run(tick_interval).await;
    });

    let router = api::router()
        .with_state(app_state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    info!(addr = %cfg.listen_addr, "starting smart-geyser-service");
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn log_startup_config(cfg: &ServiceConfig, config_path: &std::path::Path) {
    info!(
        "=== Smart Geyser Controller v{} ===",
        env!("CARGO_PKG_VERSION")
    );
    info!(path = %config_path.display(), "config loaded");
    info!(
        addr = %cfg.listen_addr,
        tick_secs = cfg.tick_interval_secs,
        data_dir = %cfg.data_dir.display(),
        "service"
    );
    info!(
        setpoint_c = cfg.engine.setpoint_c,
        hysteresis_c = cfg.engine.hysteresis_c,
        preheat_threshold = cfg.engine.preheat_threshold,
        late_use_threshold = cfg.engine.late_use_threshold,
        cutoff_buffer_min = cfg.engine.cutoff_buffer_min,
        safety_margin_min = cfg.engine.safety_margin_min,
        legionella_interval_days = cfg.engine.legionella_interval_days,
        decay_factor = cfg.engine.decay_factor,
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
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
    }
}
