use std::collections::HashSet;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::models::GeyserState;
use crate::system::HeatingSystem;

// ---------------------------------------------------------------------------
// GeyserCapability
// ---------------------------------------------------------------------------

/// A discrete capability that a `GeyserProvider` implementation may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GeyserCapability {
    /// Provider can read the tank temperature.
    TankTemp,
    /// Provider can read the solar-collector temperature.
    CollectorTemp,
    /// Provider can command the circulation pump on/off.
    PumpControl,
    /// Provider can command the electric element on/off.
    ElementControl,
    /// Provider supports a manual boost mode distinct from element on/off.
    BoostControl,
    /// Provider can report hardware fault status.
    FaultStatus,
}

/// Convenience alias for the capability set returned by a `GeyserProvider`.
pub type GeyserCapabilities = HashSet<GeyserCapability>;

// ---------------------------------------------------------------------------
// GeyserProvider trait
// ---------------------------------------------------------------------------

/// Abstraction over the physical geyser hardware.
///
/// Implementations live in `smart-geyser-providers`; this crate only defines
/// the trait contract. All methods are async to accommodate network-backed
/// providers (Geyserwala REST, MQTT, HA entity).
#[async_trait]
pub trait GeyserProvider: Send + Sync {
    /// Fetch a fresh snapshot of the geyser's hardware state.
    async fn get_state(&self) -> anyhow::Result<GeyserState>;

    /// Command the electric element (or heat-pump compressor) on or off.
    async fn set_element(&self, on: bool) -> anyhow::Result<()>;

    /// Command the solar-thermal circulation pump on or off.
    async fn set_pump(&self, on: bool) -> anyhow::Result<()>;

    /// Return the set of capabilities this provider instance supports.
    fn capabilities(&self) -> GeyserCapabilities;

    /// A human-readable name for this provider (used in logs and the API).
    fn name(&self) -> &'static str;

    /// The heating-system topology this provider is attached to.
    fn system(&self) -> HeatingSystem;
}

// ---------------------------------------------------------------------------
// MockGeyserProvider (test only)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod mock {
    use super::*;

    pub struct MockGeyserProvider {
        state: GeyserState,
        capabilities: GeyserCapabilities,
        system: HeatingSystem,
        name: &'static str,
        element_calls: std::sync::Mutex<Vec<bool>>,
        pump_calls: std::sync::Mutex<Vec<bool>>,
    }

    impl MockGeyserProvider {
        pub fn new(
            state: GeyserState,
            capabilities: GeyserCapabilities,
            system: HeatingSystem,
            name: &'static str,
        ) -> Self {
            Self {
                state,
                capabilities,
                system,
                name,
                element_calls: std::sync::Mutex::new(Vec::new()),
                pump_calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub fn element_calls(&self) -> Vec<bool> {
            self.element_calls.lock().unwrap().clone()
        }

        pub fn pump_calls(&self) -> Vec<bool> {
            self.pump_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl GeyserProvider for MockGeyserProvider {
        async fn get_state(&self) -> anyhow::Result<GeyserState> {
            Ok(self.state.clone())
        }

        async fn set_element(&self, on: bool) -> anyhow::Result<()> {
            self.element_calls.lock().unwrap().push(on);
            Ok(())
        }

        async fn set_pump(&self, on: bool) -> anyhow::Result<()> {
            self.pump_calls.lock().unwrap().push(on);
            Ok(())
        }

        fn capabilities(&self) -> GeyserCapabilities {
            self.capabilities.clone()
        }

        fn name(&self) -> &'static str {
            self.name
        }

        fn system(&self) -> HeatingSystem {
            self.system
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    use super::mock::MockGeyserProvider;
    use super::*;
    use crate::system::HeatingSystem;

    fn sample_state() -> GeyserState {
        GeyserState {
            timestamp: Utc::now(),
            tank_temp_c: 55.0,
            collector_temp_c: None,
            pump_active: None,
            heating_active: false,
            element_kw: 3.0,
            tank_volume_l: 150.0,
        }
    }

    fn electric_only_caps() -> GeyserCapabilities {
        [GeyserCapability::TankTemp, GeyserCapability::ElementControl]
            .into_iter()
            .collect()
    }

    #[tokio::test]
    async fn mock_provider_satisfies_trait() {
        let state = sample_state();
        let caps = electric_only_caps();
        let provider = MockGeyserProvider::new(
            state.clone(),
            caps.clone(),
            HeatingSystem::ElectricOnly,
            "test-electric",
        );

        assert_eq!(provider.get_state().await.unwrap(), state);
        assert_eq!(provider.capabilities(), caps);
        assert_eq!(provider.system(), HeatingSystem::ElectricOnly);
        assert_eq!(provider.name(), "test-electric");
    }

    #[tokio::test]
    async fn set_element_records_calls() {
        let provider = MockGeyserProvider::new(
            sample_state(),
            electric_only_caps(),
            HeatingSystem::ElectricOnly,
            "test",
        );

        provider.set_element(true).await.unwrap();
        provider.set_element(false).await.unwrap();

        assert_eq!(provider.element_calls(), vec![true, false]);
    }

    #[tokio::test]
    async fn set_pump_records_calls() {
        let provider = MockGeyserProvider::new(
            sample_state(),
            electric_only_caps(),
            HeatingSystem::ElectricOnly,
            "test",
        );

        provider.set_pump(true).await.unwrap();
        provider.set_pump(false).await.unwrap();

        assert_eq!(provider.pump_calls(), vec![true, false]);
    }

    #[tokio::test]
    async fn electric_only_provider_has_no_pump_capability() {
        let provider = MockGeyserProvider::new(
            sample_state(),
            electric_only_caps(),
            HeatingSystem::ElectricOnly,
            "test",
        );

        let caps = provider.capabilities();
        assert!(!caps.contains(&GeyserCapability::PumpControl));
    }
}
