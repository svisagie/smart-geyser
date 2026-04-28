//! The `HeatingSystem` taxonomy.
//!
//! Each variant captures everything the engines need to know about the
//! physical heating hardware. The model deliberately stays small —
//! anything provider-specific lives outside this crate.

use serde::{Deserialize, Serialize};

/// Voltage class for the solar-thermal circulation pump on a
/// `SolarPumped` system. Affects nothing in core today (the engines treat
/// the pump as opaque) but downstream providers and the service surface
/// it as configuration metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PumpVoltage {
    /// Low-voltage DC pump (typically driven directly by a small PV panel
    /// or a 12 V bus).
    Dc12V,
    /// Mains-AC pump.
    Ac220V,
}

/// What kind of heating hardware the controller is driving.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HeatingSystem {
    /// Resistive electric element only — no solar collector, no heat
    /// pump. Treats the element at COP 1.0.
    ElectricOnly,

    /// Resistive element plus a pumped solar-thermal collector. The pump
    /// is controlled separately; the collector contributes thermal energy
    /// passively, so the *electric-side* COP stays at 1.0 for the purposes
    /// of lead-time calculations.
    SolarPumped {
        /// Voltage class of the circulation pump.
        pump_voltage: PumpVoltage,
    },

    /// Heat pump. `cop_nominal` is the nameplate / spec COP, used as a
    /// fallback whenever a live measurement is not available.
    /// `live_cop`, when present, is a freshly measured value and takes
    /// precedence in `effective_cop`.
    HeatPump {
        /// Nameplate / spec coefficient-of-performance.
        cop_nominal: f32,
        /// Most recent measured COP, if the provider supplies one.
        live_cop: Option<f32>,
    },
}

impl HeatingSystem {
    /// The COP to use for thermal-energy-to-wall-clock-time calculations.
    ///
    /// * `ElectricOnly` and `SolarPumped` always return `1.0` — the
    ///   electric element is purely resistive in both cases.
    /// * `HeatPump` returns `live_cop` when populated, falling back to
    ///   `cop_nominal`.
    #[must_use]
    pub fn effective_cop(&self) -> f32 {
        match *self {
            Self::ElectricOnly | Self::SolarPumped { .. } => 1.0,
            Self::HeatPump {
                cop_nominal,
                live_cop,
            } => live_cop.unwrap_or(cop_nominal),
        }
    }

    /// Whether this system has a pumped solar-thermal collector loop.
    #[must_use]
    pub fn is_solar_pumped(&self) -> bool {
        matches!(self, Self::SolarPumped { .. })
    }

    /// Whether this system uses a heat pump as the primary heater.
    #[must_use]
    pub fn is_heat_pump(&self) -> bool {
        matches!(self, Self::HeatPump { .. })
    }

    /// Whether this system has only a resistive electric element and
    /// nothing else.
    #[must_use]
    pub fn is_electric_only(&self) -> bool {
        matches!(self, Self::ElectricOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // --- effective_cop ----------------------------------------------------

    #[test]
    fn electric_only_cop_is_one() {
        assert_eq!(HeatingSystem::ElectricOnly.effective_cop(), 1.0);
    }

    #[test]
    fn solar_pumped_cop_is_one_dc() {
        let s = HeatingSystem::SolarPumped {
            pump_voltage: PumpVoltage::Dc12V,
        };
        assert_eq!(s.effective_cop(), 1.0);
    }

    #[test]
    fn solar_pumped_cop_is_one_ac() {
        let s = HeatingSystem::SolarPumped {
            pump_voltage: PumpVoltage::Ac220V,
        };
        assert_eq!(s.effective_cop(), 1.0);
    }

    #[test]
    fn heat_pump_uses_nominal_when_no_live_value() {
        let s = HeatingSystem::HeatPump {
            cop_nominal: 3.5,
            live_cop: None,
        };
        assert!((s.effective_cop() - 3.5).abs() < f32::EPSILON);
    }

    #[test]
    fn heat_pump_live_cop_overrides_nominal() {
        let s = HeatingSystem::HeatPump {
            cop_nominal: 3.5,
            live_cop: Some(2.8),
        };
        assert!((s.effective_cop() - 2.8).abs() < f32::EPSILON);
    }

    // --- predicate helpers ------------------------------------------------

    #[test]
    fn is_solar_pumped_predicate() {
        assert!(!HeatingSystem::ElectricOnly.is_solar_pumped());
        assert!(HeatingSystem::SolarPumped {
            pump_voltage: PumpVoltage::Dc12V,
        }
        .is_solar_pumped());
        assert!(!HeatingSystem::HeatPump {
            cop_nominal: 3.5,
            live_cop: None,
        }
        .is_solar_pumped());
    }

    #[test]
    fn is_heat_pump_predicate() {
        assert!(HeatingSystem::HeatPump {
            cop_nominal: 3.5,
            live_cop: None,
        }
        .is_heat_pump());
        assert!(!HeatingSystem::ElectricOnly.is_heat_pump());
    }

    #[test]
    fn is_electric_only_predicate() {
        assert!(HeatingSystem::ElectricOnly.is_electric_only());
        assert!(!HeatingSystem::SolarPumped {
            pump_voltage: PumpVoltage::Ac220V,
        }
        .is_electric_only());
    }

    // --- serde ------------------------------------------------------------

    fn roundtrip<T>(value: &T) -> T
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        let json = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&json).expect("deserialize")
    }

    #[test]
    fn heating_system_serde_roundtrip_all_variants() {
        for s in [
            HeatingSystem::ElectricOnly,
            HeatingSystem::SolarPumped {
                pump_voltage: PumpVoltage::Dc12V,
            },
            HeatingSystem::SolarPumped {
                pump_voltage: PumpVoltage::Ac220V,
            },
            HeatingSystem::HeatPump {
                cop_nominal: 3.5,
                live_cop: None,
            },
            HeatingSystem::HeatPump {
                cop_nominal: 3.5,
                live_cop: Some(2.9),
            },
        ] {
            assert_eq!(s, roundtrip(&s));
        }
    }

    #[test]
    fn pump_voltage_serde_roundtrip() {
        for v in [PumpVoltage::Dc12V, PumpVoltage::Ac220V] {
            assert_eq!(v, roundtrip(&v));
        }
    }
}
