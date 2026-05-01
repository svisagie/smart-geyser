//! `GeyserwalaMqttProvider` — reactive Geyserwala Connect integration via MQTT.
//!
//! State is received immediately via MQTT stat topics; commands are published
//! to cmnd topics. This is significantly more reactive than the REST provider
//! because there is no polling — the device pushes updates as they happen.
//!
//! Topic layout (default Geyserwala template):
//!   stat: `{topic_prefix}/stat/{device_id}/tank-temp`
//!   cmnd: `{topic_prefix}/cmnd/{device_id}/external-demand`
//!
//! Boolean payloads: `ON` / `OFF`
//! Numeric payloads: integers (e.g. `60` for 60°C setpoint)

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use chrono::Utc;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use smart_geyser_core::models::GeyserState;
use smart_geyser_core::provider::{GeyserCapabilities, GeyserCapability, GeyserProvider};
use smart_geyser_core::system::HeatingSystem;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GeyserwalaMqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    /// Topic prefix before `stat`/`cmnd`, e.g. `"geyserwala"`.
    pub topic_prefix: String,
    /// Device identifier used in topics (MAC address, e.g. `"AABBCCDDEEFF"`).
    pub device_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub element_kw: f32,
    pub tank_volume_l: f32,
}

impl Default for GeyserwalaMqttConfig {
    fn default() -> Self {
        Self {
            broker_host: "localhost".to_string(),
            broker_port: 1883,
            topic_prefix: "geyserwala".to_string(),
            device_id: String::new(),
            username: None,
            password: None,
            element_kw: 3.0,
            tank_volume_l: 150.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state updated by the MQTT subscription loop
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct MqttGeyserState {
    tank_temp_c: Option<f32>,
    collector_temp_c: Option<f32>,
    element_demand: Option<bool>,
    pump_status: Option<bool>,
    setpoint_c: Option<f32>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct GeyserwalaMqttProvider {
    config: GeyserwalaMqttConfig,
    state: Arc<RwLock<MqttGeyserState>>,
    client: AsyncClient,
}

impl GeyserwalaMqttProvider {
    /// Connect to the MQTT broker, subscribe to stat topics, and start the
    /// background receive loop. Returns when the first subscription ACK arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial subscription cannot be sent.
    pub async fn new(config: GeyserwalaMqttConfig) -> anyhow::Result<Self> {
        let mut opts = MqttOptions::new("smart-geyser", &config.broker_host, config.broker_port);
        opts.set_keep_alive(Duration::from_secs(30));
        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            opts.set_credentials(user, pass);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 32);

        let wildcard = format!("{}/stat/{}/#", config.topic_prefix, config.device_id);
        client
            .subscribe(&wildcard, QoS::AtLeastOnce)
            .await
            .context("failed to subscribe to Geyserwala MQTT stat topics")?;
        info!(topic = %wildcard, "subscribed to Geyserwala MQTT stat topics");

        let state: Arc<RwLock<MqttGeyserState>> = Arc::new(RwLock::new(MqttGeyserState::default()));
        let state_bg = Arc::clone(&state);
        let stat_prefix = format!("{}/stat/{}/", config.topic_prefix, config.device_id);

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        let field = p.topic.strip_prefix(&stat_prefix).unwrap_or("").to_string();
                        let raw = match std::str::from_utf8(&p.payload) {
                            Ok(s) => s.trim().to_string(),
                            Err(_) => continue,
                        };
                        debug!(field = %field, payload = %raw, "MQTT stat received");
                        let mut s = state_bg.write().await;
                        match field.as_str() {
                            "tank-temp" => s.tank_temp_c = raw.parse::<f32>().ok(),
                            "collector-temp" => s.collector_temp_c = raw.parse::<f32>().ok(),
                            "element-demand" => {
                                s.element_demand = Some(raw.eq_ignore_ascii_case("on"));
                            }
                            "pump-status" => {
                                s.pump_status = Some(raw.eq_ignore_ascii_case("on"));
                            }
                            "setpoint" => s.setpoint_c = raw.parse::<f32>().ok(),
                            _ => {}
                        }
                    }
                    Err(e) => {
                        warn!("MQTT connection error: {e:#}; retrying in 5 s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            config,
            state,
            client,
        })
    }

    fn cmnd(&self, field: &str) -> String {
        format!(
            "{}/cmnd/{}/{}",
            self.config.topic_prefix, self.config.device_id, field
        )
    }

    async fn publish_bool(&self, field: &str, on: bool) -> anyhow::Result<()> {
        let payload = if on { "ON" } else { "OFF" };
        self.client
            .publish(self.cmnd(field), QoS::AtLeastOnce, false, payload)
            .await
            .with_context(|| format!("MQTT publish {field} failed"))
    }
}

// ---------------------------------------------------------------------------
// GeyserProvider impl
// ---------------------------------------------------------------------------

#[async_trait]
impl GeyserProvider for GeyserwalaMqttProvider {
    async fn get_state(&self) -> anyhow::Result<GeyserState> {
        let s = self.state.read().await;
        let tank_temp_c = s
            .tank_temp_c
            .ok_or_else(|| anyhow::anyhow!("tank temperature not yet received via MQTT"))?;
        Ok(GeyserState {
            timestamp: Utc::now(),
            tank_temp_c,
            collector_temp_c: s.collector_temp_c,
            pump_active: s.pump_status,
            heating_active: s.element_demand.unwrap_or(false),
            element_kw: self.config.element_kw,
            tank_volume_l: self.config.tank_volume_l,
        })
    }

    async fn set_element(&self, on: bool) -> anyhow::Result<()> {
        info!(element_on = on, topic = %self.cmnd("external-demand"), "MQTT publish external-demand");
        self.publish_bool("external-demand", on).await
    }

    async fn set_boost(&self, on: bool) -> anyhow::Result<()> {
        info!(boost_on = on, topic = %self.cmnd("boost-demand"), "MQTT publish boost-demand");
        self.publish_bool("boost-demand", on).await
    }

    async fn get_setpoint(&self) -> anyhow::Result<Option<f32>> {
        Ok(self.state.read().await.setpoint_c)
    }

    async fn set_setpoint(&self, temp_c: f32) -> anyhow::Result<()> {
        let topic = self.cmnd("setpoint");
        #[allow(clippy::cast_possible_truncation)]
        let payload = format!("{}", temp_c as i32);
        info!(setpoint_c = temp_c, topic = %topic, "MQTT publish setpoint");
        self.client
            .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
            .await
            .context("MQTT publish setpoint failed")
    }

    async fn set_pump(&self, _on: bool) -> anyhow::Result<()> {
        Ok(())
    }

    fn capabilities(&self) -> GeyserCapabilities {
        HashSet::from([
            GeyserCapability::TankTemp,
            GeyserCapability::CollectorTemp,
            GeyserCapability::ElementControl,
            GeyserCapability::BoostControl,
            GeyserCapability::SetpointControl,
        ])
    }

    fn name(&self) -> &'static str {
        "Geyserwala Connect (MQTT)"
    }

    fn system(&self) -> HeatingSystem {
        HeatingSystem::SolarPumped
    }
}
