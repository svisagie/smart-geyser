//! Service configuration — bootstrap params loaded from options.json/TOML,
//! everything else managed via the REST API and persisted in the overlay file.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use smart_geyser_core::models::{EngineConfig, OpportunityConfig, SolarWindow};
use smart_geyser_core::system::HeatingSystem;
use smart_geyser_providers::geyserwala::GeyserwalaConfig;
use smart_geyser_providers::geyserwala_mqtt::GeyserwalaMqttConfig;

// ---------------------------------------------------------------------------
// Engine settings (stored in the overlay, separate from core EngineConfig so
// we don't serialise internal fields like `system` or v2 opportunity settings)
// ---------------------------------------------------------------------------

/// User-configurable engine parameters.  Defaults match spec §9.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSettings {
    #[serde(default = "default_setpoint_c")]
    pub setpoint_c: f32,
    #[serde(default = "default_hysteresis_c")]
    pub hysteresis_c: f32,
    #[serde(default = "default_preheat_threshold")]
    pub preheat_threshold: f32,
    #[serde(default = "default_late_use_threshold")]
    pub late_use_threshold: f32,
    #[serde(default = "default_cutoff_buffer_min")]
    pub cutoff_buffer_min: u32,
    #[serde(default = "default_safety_margin_min")]
    pub safety_margin_min: u32,
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f32,
    #[serde(default = "default_legionella_interval_days")]
    pub legionella_interval_days: u32,
    #[serde(default = "default_tick_interval")]
    pub tick_interval_secs: u32,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            setpoint_c: default_setpoint_c(),
            hysteresis_c: default_hysteresis_c(),
            preheat_threshold: default_preheat_threshold(),
            late_use_threshold: default_late_use_threshold(),
            cutoff_buffer_min: default_cutoff_buffer_min(),
            safety_margin_min: default_safety_margin_min(),
            decay_factor: default_decay_factor(),
            legionella_interval_days: default_legionella_interval_days(),
            tick_interval_secs: default_tick_interval(),
        }
    }
}

impl EngineSettings {
    /// Build a full `EngineConfig` by combining stored settings with the
    /// runtime system type (derived from the provider) and v2 defaults.
    pub fn to_engine_config(&self, system: HeatingSystem) -> EngineConfig {
        EngineConfig {
            system,
            setpoint_c: self.setpoint_c,
            hysteresis_c: self.hysteresis_c,
            preheat_threshold: self.preheat_threshold,
            late_use_threshold: self.late_use_threshold,
            cutoff_buffer_min: self.cutoff_buffer_min,
            safety_margin_min: self.safety_margin_min,
            decay_factor: self.decay_factor,
            legionella_interval_days: self.legionella_interval_days,
            opportunity: None::<OpportunityConfig>,
            solar_window: None::<SolarWindow>,
        }
    }

    /// Build `EngineSettings` from a runtime `EngineConfig` (strips `system`
    /// and v2 fields — used when reading live config back for the overlay).
    pub fn from_engine_config(cfg: &EngineConfig, tick_interval_secs: u32) -> Self {
        Self {
            setpoint_c: cfg.setpoint_c,
            hysteresis_c: cfg.hysteresis_c,
            preheat_threshold: cfg.preheat_threshold,
            late_use_threshold: cfg.late_use_threshold,
            cutoff_buffer_min: cfg.cutoff_buffer_min,
            safety_margin_min: cfg.safety_margin_min,
            decay_factor: cfg.decay_factor,
            legionella_interval_days: cfg.legionella_interval_days,
            tick_interval_secs,
        }
    }
}

fn default_setpoint_c() -> f32 { 60.0 }
fn default_hysteresis_c() -> f32 { 4.0 }
fn default_preheat_threshold() -> f32 { 0.40 }
fn default_late_use_threshold() -> f32 { 0.15 }
fn default_cutoff_buffer_min() -> u32 { 30 }
fn default_safety_margin_min() -> u32 { 20 }
fn default_decay_factor() -> f32 { 0.995 }
fn default_legionella_interval_days() -> u32 { 7 }
fn default_tick_interval() -> u32 { 60 }

// ---------------------------------------------------------------------------
// Service overlay — the single file that the API writes to persist all
// user-supplied settings that are not bootstrap parameters.
// ---------------------------------------------------------------------------

/// Persistent settings managed exclusively via the REST API.
/// Written to `<data_dir>/provider-config.json` by the options flow endpoints.
/// Both `POST /api/provider-config` and `POST /api/engine-config` read, update
/// their own section, and save — preserving the other section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceOverlay {
    /// Provider hardware config; `None` until first configuration.
    pub geyser: Option<GeyserProviderConfig>,
    /// Engine / scheduler settings; uses spec defaults when absent.
    #[serde(default)]
    pub engine: EngineSettings,
    /// When true the scheduler observes but never actuates the element.
    /// Persisted so the mode survives restarts.
    #[serde(default)]
    pub read_only_mode: bool,
}

impl ServiceOverlay {
    /// Load from JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("invalid service overlay in {}", path.display()))
    }

    /// Persist to JSON file (pretty-printed for human readability).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self).context("serialisation failed")?;
        std::fs::write(path, json).with_context(|| format!("cannot write {}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// Bootstrap-only config (listen address + data directory)
// ---------------------------------------------------------------------------

/// Minimal bootstrap configuration loaded from `options.json` or `config.toml`.
/// Everything else is managed via the REST API (stored in `ServiceOverlay`).
///
/// Overridable via environment variables:
/// - `SMART_GEYSER_LISTEN_ADDR`
/// - `SMART_GEYSER_DATA_DIR`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,

    #[serde(default)]
    pub data_dir: PathBuf,
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

impl ServiceConfig {
    /// Load from a TOML or JSON file (detected by extension).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the content is invalid.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        let mut cfg: Self = if path.extension().and_then(|e| e.to_str()) == Some("json") {
            serde_json::from_str(&raw).context("invalid config JSON")?
        } else {
            toml::from_str(&raw).context("invalid config TOML")?
        };
        if let Ok(v) = std::env::var("SMART_GEYSER_LISTEN_ADDR") {
            cfg.listen_addr = v
                .parse()
                .context("SMART_GEYSER_LISTEN_ADDR is not a valid SocketAddr")?;
        }
        if let Ok(v) = std::env::var("SMART_GEYSER_DATA_DIR") {
            cfg.data_dir = PathBuf::from(v);
        }
        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Provider config (tagged enum — only Geyserwala for v1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeyserProviderConfig {
    Geyserwala(GeyserwalaTomlConfig),
    #[serde(rename = "geyserwala_mqtt")]
    GeyserwalaaMqtt(GeyserwalaaMqttTomlConfig),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeyserwalaTomlConfig {
    pub base_url: String,
    pub token: Option<String>,
    #[serde(default = "default_element_kw")]
    pub element_kw: f32,
    #[serde(default = "default_tank_volume_l")]
    pub tank_volume_l: f32,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_element_kw() -> f32 { 3.0 }
fn default_tank_volume_l() -> f32 { 150.0 }
fn default_timeout_secs() -> u64 { 10 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeyserwalaaMqttTomlConfig {
    pub broker_host: String,
    #[serde(default = "default_mqtt_port")]
    pub broker_port: u16,
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    pub device_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_element_kw")]
    pub element_kw: f32,
    #[serde(default = "default_tank_volume_l")]
    pub tank_volume_l: f32,
}

fn default_mqtt_port() -> u16 { 1883 }
fn default_topic_prefix() -> String { "geyserwala".to_string() }

impl From<GeyserwalaaMqttTomlConfig> for GeyserwalaMqttConfig {
    fn from(c: GeyserwalaaMqttTomlConfig) -> Self {
        Self {
            broker_host: c.broker_host,
            broker_port: c.broker_port,
            topic_prefix: c.topic_prefix,
            device_id: c.device_id,
            username: c.username,
            password: c.password,
            element_kw: c.element_kw,
            tank_volume_l: c.tank_volume_l,
        }
    }
}

impl From<GeyserwalaTomlConfig> for GeyserwalaConfig {
    fn from(c: GeyserwalaTomlConfig) -> Self {
        Self {
            base_url: c.base_url,
            token: c.token,
            element_kw: c.element_kw,
            tank_volume_l: c.tank_volume_l,
            timeout_secs: c.timeout_secs,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_gives_defaults() {
        let cfg: ServiceConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.listen_addr.port(), 8080);
    }

    #[test]
    fn listen_addr_override_parses() {
        let cfg: ServiceConfig = toml::from_str(r#"listen_addr = "0.0.0.0:9090""#).unwrap();
        assert_eq!(cfg.listen_addr.port(), 9090);
    }

    #[test]
    fn json_options_parses() {
        let json = r#"{"listen_addr": "0.0.0.0:8080", "data_dir": "/data"}"#;
        let cfg: ServiceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.listen_addr.port(), 8080);
        assert_eq!(cfg.data_dir.as_os_str(), "/data");
    }

    #[test]
    fn engine_settings_defaults_match_spec() {
        let e = EngineSettings::default();
        assert_eq!(e.setpoint_c, 60.0);
        assert_eq!(e.hysteresis_c, 4.0);
        assert!((e.preheat_threshold - 0.40).abs() < f32::EPSILON);
        assert!((e.late_use_threshold - 0.15).abs() < f32::EPSILON);
        assert_eq!(e.cutoff_buffer_min, 30);
        assert_eq!(e.safety_margin_min, 20);
        assert!((e.decay_factor - 0.995).abs() < f32::EPSILON);
        assert_eq!(e.legionella_interval_days, 7);
        assert_eq!(e.tick_interval_secs, 60);
    }

    #[test]
    fn service_overlay_empty_json_gives_defaults() {
        let overlay: ServiceOverlay = serde_json::from_str("{}").unwrap();
        assert!(overlay.geyser.is_none());
        assert_eq!(overlay.engine.tick_interval_secs, 60);
    }

    #[test]
    fn service_overlay_roundtrip() {
        let overlay = ServiceOverlay {
            geyser: Some(GeyserProviderConfig::Geyserwala(GeyserwalaTomlConfig {
                base_url: "http://192.168.1.50".to_string(),
                token: None,
                element_kw: 3.0,
                tank_volume_l: 150.0,
                timeout_secs: 10,
            })),
            engine: EngineSettings { setpoint_c: 65.0, ..EngineSettings::default() },
            read_only_mode: false,
        };
        let json = serde_json::to_string(&overlay).unwrap();
        let back: ServiceOverlay = serde_json::from_str(&json).unwrap();
        assert!(back.geyser.is_some());
        assert_eq!(back.engine.setpoint_c, 65.0);
    }
}
