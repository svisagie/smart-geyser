//! `PVSystemProvider` trait — spec §2.3.
//!
//! Defines the async trait that every PV / battery inverter adapter must
//! implement. The trait is completely independent of the geyser provider: a
//! system with only battery SOC still satisfies the minimum contract.
//!
//! Concrete implementations live in `smart-geyser-providers`. This module
//! contains only the trait definition and, under `#[cfg(test)]`, a
//! configurable mock provider used by unit tests throughout this crate.

use async_trait::async_trait;

use crate::models::{PVCapabilities, PVSystemState};

// ---------------------------------------------------------------------------
// PVSystemProvider trait (spec §2.3)
// ---------------------------------------------------------------------------

/// Async interface for querying the state of a PV and battery system.
///
/// Only `battery_soc_pct` is required by the contract; providers populate
/// every `PVSystemState` field they are able to and report that via
/// [`PVSystemProvider::capabilities`]. The opportunity engine degrades
/// gracefully through Path A → B → C depending on which capabilities are
/// available (spec §3.2).
///
/// All implementations must be `Send + Sync` so they can be shared across
/// async tasks via `Arc<dyn PVSystemProvider>`.
#[async_trait]
pub trait PVSystemProvider: Send + Sync {
    /// Fetch the latest PV/battery snapshot from the hardware or upstream API.
    async fn get_pv_state(&self) -> anyhow::Result<PVSystemState>;

    /// Report which optional `PVSystemState` fields this provider populates.
    ///
    /// The opportunity engine calls this once at startup to choose the best
    /// available trigger path. Implementations should return a stable,
    /// allocation-free set — calling this in a hot loop is acceptable.
    fn capabilities(&self) -> PVCapabilities;

    /// Human-readable provider identifier used in logs and diagnostics.
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// MockPVProvider (test-only)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod mock {
    use std::collections::HashSet;

    use super::*;
    use chrono::Utc;

    /// Configurable in-process PV provider for unit tests.
    ///
    /// Created either with full control via [`MockPVProvider::new`] or with the
    /// convenience constructor [`MockPVProvider::soc_only`] for the minimum
    /// viable state (battery SOC only, no optional capabilities).
    pub struct MockPVProvider {
        state: PVSystemState,
        capabilities: PVCapabilities,
        name: &'static str,
    }

    impl MockPVProvider {
        /// Create a provider that returns `state` and advertises `capabilities`.
        pub fn new(state: PVSystemState, capabilities: PVCapabilities, name: &'static str) -> Self {
            Self {
                state,
                capabilities,
                name,
            }
        }

        /// Create a minimal provider with only `battery_soc_pct` populated.
        ///
        /// All optional `PVSystemState` fields are `None` and the capability
        /// set is empty — exactly what a SOC-only inverter integration would
        /// expose.
        pub fn soc_only(soc_pct: f32) -> Self {
            let state = PVSystemState {
                timestamp: Utc::now(),
                battery_soc_pct: soc_pct,
                pv_power_w: None,
                grid_power_w: None,
                battery_power_w: None,
                load_power_w: None,
                battery_capacity_kwh: None,
            };
            Self::new(state, HashSet::new(), "mock-soc-only")
        }
    }

    #[async_trait]
    impl PVSystemProvider for MockPVProvider {
        async fn get_pv_state(&self) -> anyhow::Result<PVSystemState> {
            Ok(self.state.clone())
        }

        fn capabilities(&self) -> PVCapabilities {
            self.capabilities.clone()
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::mock::MockPVProvider;
    use super::*;
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    use crate::models::{PVCapabilities, PVCapability, PVSystemState};

    fn sample_state_full() -> PVSystemState {
        PVSystemState {
            timestamp: Utc::now(),
            battery_soc_pct: 96.0,
            pv_power_w: Some(4200.0),
            grid_power_w: Some(-850.0),
            battery_power_w: Some(50.0),
            load_power_w: Some(900.0),
            battery_capacity_kwh: Some(14.4),
        }
    }

    fn all_capabilities() -> PVCapabilities {
        [
            PVCapability::PvPower,
            PVCapability::GridPower,
            PVCapability::BatteryPower,
            PVCapability::LoadPower,
            PVCapability::BatteryCapacity,
        ]
        .into_iter()
        .collect()
    }

    // -----------------------------------------------------------------------

    /// A SOC-only provider satisfies the minimum trait contract: it returns a
    /// valid state with `battery_soc_pct` set, all optional fields `None`, and
    /// an empty capability set.
    #[tokio::test]
    async fn soc_only_provider_satisfies_contract() {
        let provider = MockPVProvider::soc_only(75.0);

        let state = provider.get_pv_state().await.expect("get_pv_state");
        assert_eq!(state.battery_soc_pct, 75.0);
        assert!(state.pv_power_w.is_none());
        assert!(state.grid_power_w.is_none());
        assert!(state.battery_power_w.is_none());
        assert!(state.load_power_w.is_none());
        assert!(state.battery_capacity_kwh.is_none());

        assert!(provider.capabilities().is_empty());
    }

    /// A provider created with all five `PVCapability` variants must advertise
    /// exactly five capabilities.
    #[tokio::test]
    async fn full_provider_exposes_all_capabilities() {
        let provider = MockPVProvider::new(sample_state_full(), all_capabilities(), "mock-full");

        assert_eq!(provider.capabilities().len(), 5);
    }

    /// Values stored in the mock round-trip unchanged through `get_pv_state`.
    #[tokio::test]
    async fn rich_provider_state_roundtrip() {
        let expected = sample_state_full();
        let provider = MockPVProvider::new(expected.clone(), all_capabilities(), "mock-roundtrip");

        let actual = provider.get_pv_state().await.expect("get_pv_state");

        assert_eq!(actual.battery_soc_pct, expected.battery_soc_pct);
        assert_eq!(actual.pv_power_w, expected.pv_power_w);
        assert_eq!(actual.grid_power_w, expected.grid_power_w);
        assert_eq!(actual.battery_power_w, expected.battery_power_w);
        assert_eq!(actual.load_power_w, expected.load_power_w);
        assert_eq!(actual.battery_capacity_kwh, expected.battery_capacity_kwh);
        assert_eq!(provider.name(), "mock-roundtrip");
    }
}
