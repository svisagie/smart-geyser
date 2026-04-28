//! Heat-calculation utilities for the smart-geyser controller.
//!
//! All formulas here are derived from first-principles thermodynamics and are
//! referenced in the project spec §6 (heat calculations). No I/O or async
//! runtime concerns belong in this module.

use crate::models::GeyserState;
use crate::system::HeatingSystem;

/// Returns the thermal energy (kWh) required to raise `volume_l` litres of
/// water by `delta_t_c` degrees Celsius.
///
/// Formula: `Q = m × c × ΔT` where `c = 4186 J/(kg·K)` and water density
/// is 1 kg/L. Joules are converted to kWh by dividing by 3 600 000.
#[must_use]
pub fn energy_to_heat_kwh(volume_l: f32, delta_t_c: f32) -> f32 {
    volume_l * delta_t_c * 4186.0 / 3_600_000.0
}

/// Returns the thermal energy (kWh) stored in `volume_l` litres of water at
/// `temp_c` above a `baseline_c` reference temperature.
///
/// Returns `0.0` when `temp_c <= baseline_c` (no stored energy above
/// baseline).
#[must_use]
pub fn thermal_energy_stored_kwh(volume_l: f32, temp_c: f32, baseline_c: f32) -> f32 {
    if temp_c <= baseline_c {
        return 0.0;
    }
    energy_to_heat_kwh(volume_l, temp_c - baseline_c)
}

/// Returns the number of minutes required to heat the tank described by
/// `state` from its current temperature to `target_temp_c`, accounting for
/// the `system`'s effective COP.
///
/// Special cases (documented behaviour):
/// * `target_temp_c <= state.tank_temp_c` → returns `0` (already at or
///   above target).
/// * `state.element_kw <= 0.0` → returns `u32::MAX` (element is offline or
///   the configuration is invalid; the caller must not schedule a heat cycle).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn heat_lead_time_minutes(
    state: &GeyserState,
    target_temp_c: f32,
    system: &HeatingSystem,
) -> u32 {
    if target_temp_c <= state.tank_temp_c {
        return 0;
    }
    if state.element_kw <= 0.0 {
        return u32::MAX;
    }
    let delta_t = target_temp_c - state.tank_temp_c;
    let energy_kwh = energy_to_heat_kwh(state.tank_volume_l, delta_t);
    let effective_power_kw = state.element_kw * system.effective_cop();
    ((energy_kwh / effective_power_kw) * 60.0).ceil() as u32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn make_state(tank_temp_c: f32, element_kw: f32, tank_volume_l: f32) -> GeyserState {
        GeyserState {
            timestamp: chrono::Utc::now(),
            tank_temp_c,
            element_kw,
            tank_volume_l,
            collector_temp_c: None,
            pump_active: None,
            heating_active: false,
        }
    }

    // --- energy_to_heat_kwh -------------------------------------------------

    #[test]
    fn energy_150l_40c_delta_approx_6_977_kwh() {
        let result = energy_to_heat_kwh(150.0, 40.0);
        assert!(
            (result - 6.977).abs() < 0.01,
            "expected ~6.977 kWh, got {result}"
        );
    }

    #[test]
    fn energy_150l_50c_delta_approx_8_721_kwh() {
        let result = energy_to_heat_kwh(150.0, 50.0);
        assert!(
            (result - 8.721).abs() < 0.01,
            "expected ~8.721 kWh, got {result}"
        );
    }

    #[test]
    fn energy_zero_volume_is_zero() {
        assert_eq!(energy_to_heat_kwh(0.0, 40.0), 0.0);
    }

    #[test]
    fn energy_zero_delta_is_zero() {
        assert_eq!(energy_to_heat_kwh(150.0, 0.0), 0.0);
    }

    // --- thermal_energy_stored_kwh ------------------------------------------

    #[test]
    fn stored_energy_150l_20_to_70_is_between_8_5_and_9_5() {
        let result = thermal_energy_stored_kwh(150.0, 70.0, 20.0);
        assert!(
            result > 8.5 && result < 9.5,
            "expected stored energy between 8.5 and 9.5 kWh, got {result}"
        );
    }

    #[test]
    fn stored_energy_temp_equal_baseline_returns_zero() {
        assert_eq!(thermal_energy_stored_kwh(150.0, 20.0, 20.0), 0.0);
    }

    #[test]
    fn stored_energy_temp_below_baseline_returns_zero() {
        assert_eq!(thermal_energy_stored_kwh(150.0, 15.0, 20.0), 0.0);
    }

    #[test]
    fn stored_energy_above_baseline_is_positive() {
        let result = thermal_energy_stored_kwh(200.0, 65.0, 15.0);
        assert!(
            result > 0.0,
            "expected positive stored energy, got {result}"
        );
    }

    // --- heat_lead_time_minutes ---------------------------------------------

    #[test]
    fn lead_time_150l_20_to_60_electric_only_is_140_min() {
        let state = make_state(20.0, 3.0, 150.0);
        let system = HeatingSystem::ElectricOnly;
        let minutes = heat_lead_time_minutes(&state, 60.0, &system);
        assert_eq!(minutes, 140);
    }

    #[test]
    fn lead_time_150l_20_to_60_heat_pump_cop_3_5_is_40_min() {
        let state = make_state(20.0, 3.0, 150.0);
        let system = HeatingSystem::HeatPump {
            cop_nominal: 3.5,
            live_cop: None,
        };
        let minutes = heat_lead_time_minutes(&state, 60.0, &system);
        assert_eq!(minutes, 40);
    }

    #[test]
    fn lead_time_target_at_current_temp_returns_zero() {
        let state = make_state(60.0, 3.0, 150.0);
        let system = HeatingSystem::ElectricOnly;
        assert_eq!(heat_lead_time_minutes(&state, 60.0, &system), 0);
    }

    #[test]
    fn lead_time_target_below_current_temp_returns_zero() {
        let state = make_state(65.0, 3.0, 150.0);
        let system = HeatingSystem::ElectricOnly;
        assert_eq!(heat_lead_time_minutes(&state, 60.0, &system), 0);
    }

    #[test]
    fn lead_time_element_kw_zero_returns_u32_max() {
        let state = make_state(20.0, 0.0, 150.0);
        let system = HeatingSystem::ElectricOnly;
        assert_eq!(heat_lead_time_minutes(&state, 60.0, &system), u32::MAX);
    }

    #[test]
    fn lead_time_element_kw_negative_returns_u32_max() {
        let state = make_state(20.0, -1.0, 150.0);
        let system = HeatingSystem::ElectricOnly;
        assert_eq!(heat_lead_time_minutes(&state, 60.0, &system), u32::MAX);
    }
}
