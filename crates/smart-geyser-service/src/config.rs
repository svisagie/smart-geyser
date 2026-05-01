//! Service configuration — loaded from TOML, individual fields overridable
//! via `SMART_GEYSER_*` environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use smart_geyser_core::models::EngineConfig;
use smart_geyser_providers::geyserwala::GeyserwalaConfig;
use smart_geyser_providers::geyserwala_mqtt::GeyserwalaMqttConfig;

// ---------------------------------------------------------------------------
// Top-level
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,

    #[serde(default = "default_tick_interval")]
    pub tick_interval_secs: u32,

    #[serde(default)]
    pub data_dir: PathBuf,

    pub geyser: GeyserProviderConfig,

    #[serde(default)]
    pub engine: EngineConfig,
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

fn default_tick_interval() -> u32 {
    60
}

// ---------------------------------------------------------------------------
// Provider config (tagged enum — only Geyserwala for v1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeyserProviderConfig {
    Geyserwala(GeyserwalaTomlConfig),
    GeyserwalaaMqtt(GeyserwalaaMqttTomlConfig),
}

/// TOML-friendly wrapper around `GeyserwalaConfig` (all fields optional
/// except `base_url`; defaults match `GeyserwalaConfig::default()`).
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

fn default_element_kw() -> f32 {
    3.0
}
fn default_tank_volume_l() -> f32 {
    150.0
}
fn default_timeout_secs() -> u64 {
    10
}

/// TOML config for the Geyserwala MQTT provider.
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

fn default_mqtt_port() -> u16 {
    1883
}
fn default_topic_prefix() -> String {
    "geyserwala".to_string()
}

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
// Loading
// ---------------------------------------------------------------------------

impl ServiceConfig {
    /// Load from a TOML or JSON file (detected by extension).
    ///
    /// JSON is used when the HA add-on framework writes `/data/options.json`.
    /// TOML is used for manual/dev deployments (`config.toml`).
    ///
    /// A handful of top-level fields can be overridden by environment variables
    /// (prefix `SMART_GEYSER_`):
    /// - `SMART_GEYSER_LISTEN_ADDR`
    /// - `SMART_GEYSER_TICK_INTERVAL_SECS`
    /// - `SMART_GEYSER_DATA_DIR`
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

        // Environment overrides.
        if let Ok(v) = std::env::var("SMART_GEYSER_LISTEN_ADDR") {
            cfg.listen_addr = v
                .parse()
                .context("SMART_GEYSER_LISTEN_ADDR is not a valid SocketAddr")?;
        }
        if let Ok(v) = std::env::var("SMART_GEYSER_TICK_INTERVAL_SECS") {
            cfg.tick_interval_secs = v
                .parse()
                .context("SMART_GEYSER_TICK_INTERVAL_SECS must be an integer")?;
        }
        if let Ok(v) = std::env::var("SMART_GEYSER_DATA_DIR") {
            cfg.data_dir = PathBuf::from(v);
        }

        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TOML: &str = r#"
[geyser]
type = "geyserwala"
base_url = "http://192.168.1.50"
"#;

    #[test]
    fn minimal_toml_parses() {
        let cfg: ServiceConfig = toml::from_str(MINIMAL_TOML).unwrap();
        match &cfg.geyser {
            GeyserProviderConfig::Geyserwala(g) => {
                assert_eq!(g.base_url, "http://192.168.1.50");
                assert_eq!(g.element_kw, 3.0);
                assert_eq!(g.tank_volume_l, 150.0);
            }
            _ => panic!("expected Geyserwala variant"),
        }
        assert_eq!(cfg.tick_interval_secs, 60);
    }

    #[test]
    fn full_toml_parses() {
        let toml = r#"
listen_addr = "0.0.0.0:9090"
tick_interval_secs = 30
data_dir = "/tmp/geyser"

[geyser]
type = "geyserwala"
base_url = "http://10.0.0.5"
token = "abc123"
element_kw = 4.0
tank_volume_l = 200.0
timeout_secs = 15

[engine]
setpoint_c = 62.0
"#;
        let cfg: ServiceConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.tick_interval_secs, 30);
        assert_eq!(cfg.engine.setpoint_c, 62.0);
        match &cfg.geyser {
            GeyserProviderConfig::Geyserwala(g) => {
                assert_eq!(g.token, Some("abc123".to_string()));
                assert_eq!(g.element_kw, 4.0);
            }
            _ => panic!("expected Geyserwala variant"),
        }
    }

    #[test]
    fn missing_geyser_section_errors() {
        let result: Result<ServiceConfig, _> = toml::from_str("tick_interval_secs = 30");
        assert!(result.is_err());
    }

    #[test]
    fn json_options_parses() {
        // Mirrors the structure HA writes to /data/options.json from addon/config.yaml.
        let json = r#"{
            "listen_addr": "0.0.0.0:8080",
            "tick_interval_secs": 60,
            "data_dir": "/data",
            "geyser": {
                "type": "geyserwala",
                "base_url": "http://192.168.1.50",
                "token": "",
                "element_kw": 3.0,
                "tank_volume_l": 150.0,
                "timeout_secs": 10
            },
            "engine": {
                "setpoint_c": 60.0,
                "hysteresis_c": 4.0
            }
        }"#;
        let cfg: ServiceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.tick_interval_secs, 60);
        assert_eq!(cfg.engine.setpoint_c, 60.0);
        match &cfg.geyser {
            GeyserProviderConfig::Geyserwala(g) => {
                assert_eq!(g.base_url, "http://192.168.1.50");
                assert_eq!(g.element_kw, 3.0);
            }
            _ => panic!("expected Geyserwala variant"),
        }
    }
}
