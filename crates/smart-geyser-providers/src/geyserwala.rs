//! `GeyserwalaProvider` — talks to the Geyserwala Connect local HTTP API.
//!
//! API reference: <https://www.thingwala.com/geyserwala/connect/integration>
//!
//! State is polled via `GET /api/value?f=<fields>`. The electric element is
//! controlled via `PATCH /api/value` with `{"external-demand": <bool>}`.
//! The circulation pump is managed automatically by the Geyserwala firmware
//! based on collector/tank temperature differential; direct pump control is
//! not exposed by this provider.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, info};

use smart_geyser_core::models::GeyserState;
use smart_geyser_core::provider::{GeyserCapabilities, GeyserCapability, GeyserProvider};
use smart_geyser_core::system::HeatingSystem;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a Geyserwala Connect device.
#[derive(Debug, Clone)]
pub struct GeyserwalaConfig {
    /// Base URL of the device, e.g. `"http://192.168.1.100"` or
    /// `"http://geyserwala.local"`.
    pub base_url: String,
    /// Bearer token, if a Local app password is configured on the device.
    pub token: Option<String>,
    /// Rated element power (kW). Static — the API does not expose live
    /// element power. Default: 3.0.
    pub element_kw: f32,
    /// Tank volume (litres). Static config. Default: 150.0.
    pub tank_volume_l: f32,
    /// HTTP request timeout. Default: 10 s.
    pub timeout_secs: u64,
}

impl Default for GeyserwalaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://geyserwala.local".to_string(),
            token: None,
            element_kw: 3.0,
            tank_volume_l: 150.0,
            timeout_secs: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Geyserwala Connect geyser provider.
pub struct GeyserwalaProvider {
    config: GeyserwalaConfig,
    client: Client,
}

impl GeyserwalaProvider {
    /// Create a new provider, building the underlying HTTP client.
    ///
    /// # Errors
    ///
    /// Returns an error if the bearer token contains invalid header characters
    /// or if the HTTP client cannot be constructed.
    pub fn new(config: GeyserwalaConfig) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(ref token) = config.token {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("Geyserwala token contains invalid header characters")?;
            headers.insert(header::AUTHORIZATION, value);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(headers)
            .build()
            .context("failed to build reqwest client")?;

        Ok(Self { config, client })
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Deserialised subset of the `/api/value` response we care about.
#[derive(Deserialize)]
struct RawState {
    #[serde(rename = "tank-temp")]
    tank_temp: f32,
    #[serde(rename = "collector-temp", default)]
    collector_temp: Option<f32>,
    #[serde(rename = "element-demand")]
    element_demand: bool,
    #[serde(rename = "pump-status", default)]
    pump_status: Option<bool>,
}

#[derive(Deserialize)]
struct RawSetpoint {
    setpoint: f32,
}

// ---------------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl GeyserProvider for GeyserwalaProvider {
    async fn get_state(&self) -> anyhow::Result<GeyserState> {
        let url = format!(
            "{}/api/value?f=tank-temp,collector-temp,element-demand,pump-status",
            self.config.base_url
        );

        let raw: RawState = self
            .client
            .get(&url)
            .send()
            .await
            .context("GET /api/value request failed")?
            .error_for_status()
            .context("Geyserwala returned an error status")?
            .json()
            .await
            .context("failed to parse Geyserwala state response")?;

        let state = GeyserState {
            timestamp: Utc::now(),
            tank_temp_c: raw.tank_temp,
            collector_temp_c: raw.collector_temp,
            pump_active: raw.pump_status,
            heating_active: raw.element_demand,
            element_kw: self.config.element_kw,
            tank_volume_l: self.config.tank_volume_l,
        };
        debug!(
            tank_temp_c = state.tank_temp_c,
            collector_temp_c = ?state.collector_temp_c,
            pump_active = ?state.pump_active,
            heating_active = state.heating_active,
            url = %url,
            "Geyserwala GET /api/value"
        );
        Ok(state)
    }

    async fn set_element(&self, on: bool) -> anyhow::Result<()> {
        let url = format!("{}/api/value", self.config.base_url);
        info!(
            element_on = on,
            payload = ?json!({ "external-demand": on }),
            url = %url,
            "Geyserwala PATCH /api/value — setting external-demand"
        );
        self.client
            .patch(&url)
            .json(&json!({ "external-demand": on }))
            .send()
            .await
            .context("PATCH /api/value request failed")?
            .error_for_status()
            .context("Geyserwala returned an error status on set_element")?;

        Ok(())
    }

    async fn get_setpoint(&self) -> anyhow::Result<Option<f32>> {
        let url = format!("{}/api/value?f=setpoint", self.config.base_url);
        let raw: RawSetpoint = self
            .client
            .get(&url)
            .send()
            .await
            .context("GET /api/value?f=setpoint request failed")?
            .error_for_status()
            .context("Geyserwala returned an error status on get_setpoint")?
            .json()
            .await
            .context("failed to parse Geyserwala setpoint response")?;
        debug!(setpoint_c = raw.setpoint, "Geyserwala GET setpoint");
        Ok(Some(raw.setpoint))
    }

    async fn set_setpoint(&self, temp_c: f32) -> anyhow::Result<()> {
        let url = format!("{}/api/value", self.config.base_url);
        info!(
            setpoint_c = temp_c,
            url = %url,
            "Geyserwala PATCH /api/value — updating setpoint"
        );
        self.client
            .patch(&url)
            .json(&json!({ "setpoint": temp_c }))
            .send()
            .await
            .context("PATCH /api/value (setpoint) request failed")?
            .error_for_status()
            .context("Geyserwala returned an error status on set_setpoint")?;
        Ok(())
    }

    async fn set_boost(&self, on: bool) -> anyhow::Result<()> {
        let url = format!("{}/api/value", self.config.base_url);
        info!(
            boost_on = on,
            url = %url,
            "Geyserwala PATCH /api/value — setting boost-demand"
        );
        self.client
            .patch(&url)
            .json(&json!({ "boost-demand": on }))
            .send()
            .await
            .context("PATCH /api/value (boost-demand) request failed")?
            .error_for_status()
            .context("Geyserwala returned an error status on set_boost")?;
        Ok(())
    }

    async fn set_pump(&self, _on: bool) -> anyhow::Result<()> {
        // The Geyserwala firmware manages the collector pump automatically
        // based on collector/tank temperature differential. The API exposes
        // pump-status as read-only; there is no write path.
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
        "Geyserwala Connect"
    }

    fn system(&self) -> HeatingSystem {
        HeatingSystem::SolarPumped
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn provider_for(server: &MockServer) -> GeyserwalaProvider {
        GeyserwalaProvider::new(GeyserwalaConfig {
            base_url: server.uri(),
            ..GeyserwalaConfig::default()
        })
        .unwrap()
    }

    // -----------------------------------------------------------------------
    // get_state
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_state_maps_all_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tank-temp": 58.5,
                "collector-temp": 74.2,
                "element-demand": false,
                "pump-status": true
            })))
            .mount(&server)
            .await;

        let state = provider_for(&server).await.get_state().await.unwrap();

        assert!((state.tank_temp_c - 58.5).abs() < 0.01);
        assert_eq!(state.collector_temp_c, Some(74.2));
        assert_eq!(state.heating_active, false);
        assert_eq!(state.pump_active, Some(true));
        assert_eq!(state.element_kw, 3.0);
        assert_eq!(state.tank_volume_l, 150.0);
    }

    #[tokio::test]
    async fn get_state_handles_absent_optional_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tank-temp": 60.0,
                "element-demand": true
            })))
            .mount(&server)
            .await;

        let state = provider_for(&server).await.get_state().await.unwrap();

        assert_eq!(state.collector_temp_c, None);
        assert_eq!(state.pump_active, None);
        assert_eq!(state.heating_active, true);
    }

    #[tokio::test]
    async fn get_state_propagates_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let result = provider_for(&server).await.get_state().await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // set_element
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn set_element_sends_external_demand_true() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"external-demand": true})),
            )
            .mount(&server)
            .await;

        provider_for(&server).await.set_element(true).await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["external-demand"], json!(true));
    }

    #[tokio::test]
    async fn set_element_sends_external_demand_false() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"external-demand": false})),
            )
            .mount(&server)
            .await;

        provider_for(&server)
            .await
            .set_element(false)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["external-demand"], json!(false));
    }

    #[tokio::test]
    async fn set_element_propagates_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let result = provider_for(&server).await.set_element(true).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // get_setpoint / set_setpoint
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_setpoint_returns_device_value() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"setpoint": 65.0})))
            .mount(&server)
            .await;

        let result = provider_for(&server).await.get_setpoint().await.unwrap();
        assert_eq!(result, Some(65.0));
    }

    #[tokio::test]
    async fn set_setpoint_sends_correct_field() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        provider_for(&server)
            .await
            .set_setpoint(62.0)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["setpoint"], json!(62.0));
    }

    // -----------------------------------------------------------------------
    // set_boost
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn set_boost_sends_boost_demand_true() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        provider_for(&server).await.set_boost(true).await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["boost-demand"], json!(true));
    }

    #[tokio::test]
    async fn set_boost_sends_boost_demand_false() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/value"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        provider_for(&server).await.set_boost(false).await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["boost-demand"], json!(false));
    }

    // -----------------------------------------------------------------------
    // set_pump
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn set_pump_is_a_no_op_no_http_call() {
        let server = MockServer::start().await;
        // No mock registered — wiremock returns 404 for unregistered routes.
        // set_pump must not make any HTTP request at all.
        provider_for(&server).await.set_pump(true).await.unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Auth
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn bearer_token_sent_in_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/value"))
            .and(wiremock::matchers::header("authorization", "Bearer secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tank-temp": 60.0,
                "element-demand": false
            })))
            .mount(&server)
            .await;

        let provider = GeyserwalaProvider::new(GeyserwalaConfig {
            base_url: server.uri(),
            token: Some("secret".to_string()),
            ..GeyserwalaConfig::default()
        })
        .unwrap();

        // If the header is missing or wrong, wiremock returns 404 → error.
        provider.get_state().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Capabilities / metadata
    // -----------------------------------------------------------------------

    #[test]
    fn capabilities_include_tank_collector_element_boost_setpoint() {
        let p = GeyserwalaProvider::new(GeyserwalaConfig::default()).unwrap();
        let caps = p.capabilities();
        assert!(caps.contains(&GeyserCapability::TankTemp));
        assert!(caps.contains(&GeyserCapability::CollectorTemp));
        assert!(caps.contains(&GeyserCapability::ElementControl));
        assert!(caps.contains(&GeyserCapability::BoostControl));
        assert!(caps.contains(&GeyserCapability::SetpointControl));
        assert!(!caps.contains(&GeyserCapability::PumpControl));
    }

    #[test]
    fn name_is_geyserwala_connect() {
        let p = GeyserwalaProvider::new(GeyserwalaConfig::default()).unwrap();
        assert_eq!(p.name(), "Geyserwala Connect");
    }

    #[test]
    fn system_is_solar_pumped() {
        let p = GeyserwalaProvider::new(GeyserwalaConfig::default()).unwrap();
        assert_eq!(p.system(), HeatingSystem::SolarPumped);
    }
}
