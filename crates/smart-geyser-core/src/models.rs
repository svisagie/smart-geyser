//! Domain models shared across the smart-geyser core.
//!
//! All numeric fields carry their unit in the field name (`_c`, `_w`,
//! `_kwh`, `_pct`, `_min`, etc.). New fields must follow that convention —
//! ambiguous numeric names are not allowed.
//!
//! Spec references:
//! * `PVSystemState` / `PVCapability` — §2.1 / §2.2
//! * `OpportunityConfig` — §3.5
//! * `SolarWindow` — §3.4
//! * `EngineConfig` — §9

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::system::HeatingSystem;

// ---------------------------------------------------------------------------
// GeyserState
// ---------------------------------------------------------------------------

/// A snapshot of the geyser's hardware state at a point in time.
///
/// Fields that depend on optional hardware (e.g. a solar collector probe or
/// a flow-driven pump indicator) are `Option`. Whether a provider populates
/// them is reported through `GeyserCapability` (defined in `provider`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeyserState {
    /// When the snapshot was captured.
    pub timestamp: DateTime<Utc>,

    /// Current tank water temperature in Celsius.
    pub tank_temp_c: f32,

    /// Solar-thermal collector temperature in Celsius. `None` for systems
    /// without a collector probe (e.g. `ElectricOnly`).
    pub collector_temp_c: Option<f32>,

    /// Whether the solar-thermal pump is currently running. `None` for
    /// systems without a controllable pump.
    pub pump_active: Option<bool>,

    /// Whether the electric element / heat-pump is currently heating.
    pub heating_active: bool,

    /// Element / heat-pump nameplate power in kilowatts. Used by the heat
    /// calculation to convert thermal energy into wall-clock time.
    pub element_kw: f32,

    /// Tank volume in litres. Combined with the temperature delta this
    /// determines stored thermal energy.
    pub tank_volume_l: f32,
}

// ---------------------------------------------------------------------------
// PVSystemState (spec §2.1)
// ---------------------------------------------------------------------------

/// A snapshot of the PV and battery system at a point in time.
///
/// Per spec §2.1 only `battery_soc_pct` is required. Provider implementations
/// populate as much of the rest as they can and advertise it via
/// `PVCapability`. The opportunity engine degrades gracefully through Path A
/// → B → C based on what's available (spec §3.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PVSystemState {
    /// When the snapshot was captured.
    pub timestamp: DateTime<Utc>,

    // --- REQUIRED ---
    /// Battery state-of-charge, 0.0 – 100.0 %.
    pub battery_soc_pct: f32,

    // --- OPTIONAL: improves opportunity heating decisions ---
    /// Current PV generation in watts.
    pub pv_power_w: Option<f32>,

    /// Grid power. Positive = importing, negative = exporting.
    pub grid_power_w: Option<f32>,

    /// Battery power. Positive = charging, negative = discharging.
    pub battery_power_w: Option<f32>,

    /// Total household load in watts.
    pub load_power_w: Option<f32>,

    /// Total usable battery capacity in kWh.
    pub battery_capacity_kwh: Option<f32>,
}

// ---------------------------------------------------------------------------
// PVCapability (spec §2.2)
// ---------------------------------------------------------------------------

/// Optional PV-system signals that a provider may expose. The opportunity
/// engine inspects this set to pick the best available trigger path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PVCapability {
    /// `pv_power_w` is populated.
    PvPower,
    /// `grid_power_w` is populated — enables export detection (Path A).
    GridPower,
    /// `battery_power_w` is populated.
    BatteryPower,
    /// `load_power_w` is populated — enables coverage calculation (Path B).
    LoadPower,
    /// `battery_capacity_kwh` is populated.
    BatteryCapacity,
}

/// Convenience alias for the capability set returned by a `PVSystemProvider`.
pub type PVCapabilities = HashSet<PVCapability>;

// ---------------------------------------------------------------------------
// OpportunityConfig (spec §3.5)
// ---------------------------------------------------------------------------

/// Tunables for the PV opportunity-heating engine. Defaults are spec-defined
/// (§3.5) and must match exactly — see `Default` impl below.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpportunityConfig {
    /// SOC % at which the battery is considered "full".
    /// Default: 95.0 — captures near-full states and avoids chasing 100 %.
    pub soc_full_threshold: f32,

    /// Minimum grid export (W, magnitude) to trigger Path A.
    /// Default: 200.0.
    pub export_floor_w: f32,

    /// Fraction of element power that must be covered by PV surplus for
    /// Path B. Default: 0.85.
    pub pv_coverage_ratio: f32,

    /// Maximum tank temperature during opportunity heating, in Celsius.
    /// Default: 70.0 — geyser as thermal battery, bounded by hardware.
    pub opportunity_max_temp_c: f32,

    /// Minimum duration (minutes) to run once started.
    /// Default: 15 — anti-cycling against transient cloud cover.
    pub min_run_minutes: u32,

    /// Minimum SOC drop (percentage points) before cancelling an active
    /// opportunity session. Default: 3.0.
    pub soc_hysteresis_pct: f32,

    /// Whether opportunity heating overrides smart-stop. Default: true —
    /// smart-stop prevents waste; PV heating is not waste (spec §3.6).
    pub override_smart_stop: bool,
}

impl Default for OpportunityConfig {
    fn default() -> Self {
        // Defaults straight from spec §3.5. Do not edit without updating
        // the spec.
        Self {
            soc_full_threshold: 95.0,
            export_floor_w: 200.0,
            pv_coverage_ratio: 0.85,
            opportunity_max_temp_c: 70.0,
            min_run_minutes: 15,
            soc_hysteresis_pct: 3.0,
            override_smart_stop: true,
        }
    }
}

// ---------------------------------------------------------------------------
// SolarWindow (spec §3.4)
// ---------------------------------------------------------------------------

/// The time-of-day window during which meaningful PV generation is expected.
///
/// Phase 1 only defines the data shape; the actual sunrise/sunset math is a
/// Phase 3 responsibility. The two methods below intentionally panic via
/// `unimplemented!()` so anything that calls them today fails loudly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolarWindow {
    /// Site latitude (decimal degrees) used for sunrise/sunset.
    pub latitude: f32,
    /// Site longitude (decimal degrees) used for sunrise/sunset.
    pub longitude: f32,
    /// Don't start an opportunity session if fewer than this many minutes
    /// remain in today's solar window. Default: 45 (spec §3.4).
    pub min_remaining_minutes: u32,
}

impl SolarWindow {
    /// Returns true if there is meaningful PV generation time remaining
    /// today.
    ///
    /// # Panics
    /// Always panics — the implementation lands in Phase 3.
    #[must_use]
    pub fn is_active(&self, _now: DateTime<Utc>) -> bool {
        unimplemented!("SolarWindow::is_active lands in Phase 3 (sunrise/sunset math)")
    }

    /// Returns the number of minutes of useful PV remaining in today's
    /// window.
    ///
    /// # Panics
    /// Always panics — the implementation lands in Phase 3.
    #[must_use]
    pub fn minutes_remaining(&self, _now: DateTime<Utc>) -> u32 {
        unimplemented!("SolarWindow::minutes_remaining lands in Phase 3")
    }
}

// ---------------------------------------------------------------------------
// EngineConfig (spec §9)
// ---------------------------------------------------------------------------

/// Top-level configuration for the engines. Defaults must match spec §9
/// exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    // --- System ---
    /// What kind of geyser hardware is being controlled.
    pub system: HeatingSystem,

    // --- Normal scheduling ---
    /// Target tank temperature for normal scheduling, °C. Default: 60.0.
    pub setpoint_c: f32,
    /// Hysteresis around the setpoint, °C. Default: 4.0.
    pub hysteresis_c: f32,
    /// Histogram-bin probability above which a pre-heat is scheduled.
    /// Default: 0.40.
    pub preheat_threshold: f32,
    /// Histogram-bin probability below which late-day usage is unlikely.
    /// Default: 0.15.
    pub late_use_threshold: f32,
    /// Buffer (minutes) before predicted use to stop heating. Default: 30.
    pub cutoff_buffer_min: u32,
    /// Safety margin (minutes) added to lead-time calculations.
    /// Default: 20.
    pub safety_margin_min: u32,
    /// Per-tick decay factor for the usage histogram. Default: 0.995.
    pub decay_factor: f32,
    /// Days between forced legionella cycles. Default: 7.
    pub legionella_interval_days: u32,

    // --- PV opportunity (optional — None disables PV integration) ---
    /// Tunables for the opportunity engine; `None` disables it entirely.
    pub opportunity: Option<OpportunityConfig>,

    // --- Solar window (used by opportunity engine) ---
    /// Site solar window; `None` falls back to 07:00–17:00 local (spec §3.4).
    pub solar_window: Option<SolarWindow>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        // Defaults straight from spec §9. Do not edit without updating the
        // spec.
        Self {
            system: HeatingSystem::ElectricOnly,
            setpoint_c: 60.0,
            hysteresis_c: 4.0,
            preheat_threshold: 0.40,
            late_use_threshold: 0.15,
            cutoff_buffer_min: 30,
            safety_margin_min: 20,
            decay_factor: 0.995,
            legionella_interval_days: 7,
            opportunity: None,
            solar_window: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    fn sample_timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 28, 14, 30, 0).unwrap()
    }

    // --- Default-value tests (spec §3.5 / §9) -----------------------------

    #[test]
    fn opportunity_config_defaults_match_spec_3_5() {
        let cfg = OpportunityConfig::default();
        assert_eq!(cfg.soc_full_threshold, 95.0);
        assert_eq!(cfg.export_floor_w, 200.0);
        assert_eq!(cfg.pv_coverage_ratio, 0.85);
        assert_eq!(cfg.opportunity_max_temp_c, 70.0);
        assert_eq!(cfg.min_run_minutes, 15);
        assert_eq!(cfg.soc_hysteresis_pct, 3.0);
        assert!(cfg.override_smart_stop);
    }

    #[test]
    fn engine_config_defaults_match_spec_9() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.system, HeatingSystem::ElectricOnly);
        assert_eq!(cfg.setpoint_c, 60.0);
        assert_eq!(cfg.hysteresis_c, 4.0);
        assert!((cfg.preheat_threshold - 0.40).abs() < f32::EPSILON);
        assert!((cfg.late_use_threshold - 0.15).abs() < f32::EPSILON);
        assert_eq!(cfg.cutoff_buffer_min, 30);
        assert_eq!(cfg.safety_margin_min, 20);
        assert!((cfg.decay_factor - 0.995).abs() < f32::EPSILON);
        assert_eq!(cfg.legionella_interval_days, 7);
        assert!(cfg.opportunity.is_none());
        assert!(cfg.solar_window.is_none());
    }

    // --- Round-trip serde tests ------------------------------------------

    fn roundtrip<T>(value: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let json = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&json).expect("deserialize")
    }

    #[test]
    fn geyser_state_serde_roundtrip() {
        let state = GeyserState {
            timestamp: sample_timestamp(),
            tank_temp_c: 48.5,
            collector_temp_c: Some(72.1),
            pump_active: Some(true),
            heating_active: false,
            element_kw: 3.0,
            tank_volume_l: 150.0,
        };
        assert_eq!(state, roundtrip(&state));
    }

    #[test]
    fn geyser_state_serde_roundtrip_minimal() {
        // ElectricOnly-style state: no collector, no pump.
        let state = GeyserState {
            timestamp: sample_timestamp(),
            tank_temp_c: 55.0,
            collector_temp_c: None,
            pump_active: None,
            heating_active: true,
            element_kw: 3.0,
            tank_volume_l: 200.0,
        };
        assert_eq!(state, roundtrip(&state));
    }

    #[test]
    fn pv_system_state_serde_roundtrip_minimum() {
        // Spec §2.4: SOC-only is the minimum viable PV state.
        let state = PVSystemState {
            timestamp: sample_timestamp(),
            battery_soc_pct: 87.4,
            pv_power_w: None,
            grid_power_w: None,
            battery_power_w: None,
            load_power_w: None,
            battery_capacity_kwh: None,
        };
        assert_eq!(state, roundtrip(&state));
    }

    #[test]
    fn pv_system_state_serde_roundtrip_full() {
        let state = PVSystemState {
            timestamp: sample_timestamp(),
            battery_soc_pct: 96.0,
            pv_power_w: Some(4200.0),
            grid_power_w: Some(-850.0),
            battery_power_w: Some(50.0),
            load_power_w: Some(900.0),
            battery_capacity_kwh: Some(14.4),
        };
        assert_eq!(state, roundtrip(&state));
    }

    #[test]
    fn pv_capability_serde_roundtrip() {
        for cap in [
            PVCapability::PvPower,
            PVCapability::GridPower,
            PVCapability::BatteryPower,
            PVCapability::LoadPower,
            PVCapability::BatteryCapacity,
        ] {
            assert_eq!(cap, roundtrip(&cap));
        }
    }

    #[test]
    fn opportunity_config_serde_roundtrip() {
        let cfg = OpportunityConfig::default();
        assert_eq!(cfg, roundtrip(&cfg));
    }

    #[test]
    fn solar_window_serde_roundtrip() {
        let w = SolarWindow {
            latitude: -33.918_861,
            longitude: 18.423_30,
            min_remaining_minutes: 45,
        };
        assert_eq!(w, roundtrip(&w));
    }

    #[test]
    fn engine_config_serde_roundtrip_default() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg, roundtrip(&cfg));
    }

    #[test]
    fn engine_config_serde_roundtrip_with_opportunity() {
        let cfg = EngineConfig {
            opportunity: Some(OpportunityConfig::default()),
            solar_window: Some(SolarWindow {
                latitude: -33.9,
                longitude: 18.4,
                min_remaining_minutes: 45,
            }),
            ..EngineConfig::default()
        };
        assert_eq!(cfg, roundtrip(&cfg));
    }

    // --- SolarWindow stub --------------------------------------------------

    #[test]
    #[should_panic(expected = "lands in Phase 3")]
    fn solar_window_is_active_is_unimplemented() {
        let w = SolarWindow {
            latitude: 0.0,
            longitude: 0.0,
            min_remaining_minutes: 45,
        };
        let _ = w.is_active(sample_timestamp());
    }

    #[test]
    #[should_panic(expected = "lands in Phase 3")]
    fn solar_window_minutes_remaining_is_unimplemented() {
        let w = SolarWindow {
            latitude: 0.0,
            longitude: 0.0,
            min_remaining_minutes: 45,
        };
        let _ = w.minutes_remaining(sample_timestamp());
    }
}
