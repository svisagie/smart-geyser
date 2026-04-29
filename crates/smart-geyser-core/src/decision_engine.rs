//! Pattern-based pre-heat scheduling and smart-stop decision engine.

use chrono::{DateTime, Duration, Utc};

use crate::heat_calc::heat_lead_time_minutes;
use crate::models::{EngineConfig, GeyserState};
use crate::pattern_store::PatternStore;
use crate::shared_state::{is_boosting, SharedState};

// ---------------------------------------------------------------------------
// DecisionIntent
// ---------------------------------------------------------------------------

/// Why an opportunity heating session was triggered (spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpportunityReason {
    /// Path A — battery full and actively exporting to grid.
    BatteryFullExporting,
    /// Path B — battery full and PV generation covers element draw.
    BatteryFullPvCoverage,
    /// Path C — battery full and solar window active (SOC-only fallback).
    BatteryFullSocOnly,
}

/// The output of a single decision-engine tick.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionIntent {
    /// No action needed.
    Idle,
    /// Pre-heat the tank to at least `until_temp_c`.
    Preheat { until_temp_c: f32 },
    /// Manual override active until `until`.
    Boost { until: DateTime<Utc> },
    /// Suppress heating — patterns show no use expected soon.
    SmartStop,
    /// PV surplus opportunity heating — heat to `target_temp_c` while free
    /// energy is available. Overrides smart-stop (spec §3.6).
    Opportunity {
        reason: OpportunityReason,
        target_temp_c: f32,
    },
}

// ---------------------------------------------------------------------------
// DecisionEngine
// ---------------------------------------------------------------------------

pub struct DecisionEngine {
    config: EngineConfig,
    pattern_store: PatternStore,
    shared: SharedState,
    /// Tracks consecutive minutes `tank_temp_c` >= 65.0 while heating is active.
    high_temp_minutes: u32,
}

impl DecisionEngine {
    #[must_use]
    pub fn new(config: EngineConfig, pattern_store: PatternStore, shared: SharedState) -> Self {
        Self {
            config,
            pattern_store,
            shared,
            high_temp_minutes: 0,
        }
    }

    /// Run one decision cycle.
    ///
    /// # Panics
    ///
    /// Panics if `boost_until` is `None` when `is_boosting` returns `true`
    /// (invariant: `is_boosting` only returns `true` when the field is `Some`).
    pub async fn tick(&mut self, state: &GeyserState, now: DateTime<Utc>) -> DecisionIntent {
        // ------------------------------------------------------------------
        // 1. Legionella tracking
        // ------------------------------------------------------------------
        if state.tank_temp_c >= 65.0 && state.heating_active {
            self.high_temp_minutes += 1;
        } else {
            self.high_temp_minutes = 0;
        }
        if self.high_temp_minutes >= 30 {
            self.shared.record_high_temp_event(now).await;
            self.high_temp_minutes = 0;
        }

        // ------------------------------------------------------------------
        // 2. Boost passthrough (highest priority)
        // ------------------------------------------------------------------
        let snap = self.shared.read().await;
        if is_boosting(&snap, now) {
            let until = snap.boost_until.unwrap();
            drop(snap);
            self.shared.set_preheat(false).await;
            self.shared.set_smart_stop(false).await;
            return DecisionIntent::Boost { until };
        }
        drop(snap);

        // ------------------------------------------------------------------
        // 3. Legionella force-heat
        // ------------------------------------------------------------------
        let snap = self.shared.read().await;
        let needs_legionella = match snap.last_high_temp_event {
            None => true,
            Some(last) => {
                (now - last).num_days() >= i64::from(self.config.legionella_interval_days)
            }
        };
        drop(snap);

        if needs_legionella && state.tank_temp_c < 65.0 {
            self.shared.set_preheat(true).await;
            self.shared.set_smart_stop(false).await;
            return DecisionIntent::Preheat { until_temp_c: 65.0 };
        }

        // ------------------------------------------------------------------
        // 4. Lead-time calculation
        // ------------------------------------------------------------------
        let lead_minutes =
            heat_lead_time_minutes(state, self.config.setpoint_c, &self.config.system)
                + self.config.safety_margin_min;
        let look_ahead = now + Duration::minutes(i64::from(lead_minutes));

        // ------------------------------------------------------------------
        // 5. Pattern queries
        // ------------------------------------------------------------------
        let prob_at_use_time = self.pattern_store.probability_at(look_ahead);
        let prob_next_buffer = self
            .pattern_store
            .probability_at(now + Duration::minutes(i64::from(self.config.cutoff_buffer_min)));

        // ------------------------------------------------------------------
        // 6. Pre-heat trigger
        // ------------------------------------------------------------------
        if prob_at_use_time >= self.config.preheat_threshold
            && state.tank_temp_c < (self.config.setpoint_c - self.config.hysteresis_c)
        {
            self.shared.set_preheat(true).await;
            self.shared.set_smart_stop(false).await;
            return DecisionIntent::Preheat {
                until_temp_c: self.config.setpoint_c,
            };
        }

        // ------------------------------------------------------------------
        // 7. Smart-stop trigger
        // ------------------------------------------------------------------
        if prob_next_buffer < self.config.late_use_threshold
            && state.tank_temp_c >= (self.config.setpoint_c - self.config.hysteresis_c)
        {
            self.shared.set_smart_stop(true).await;
            self.shared.set_preheat(false).await;
            return DecisionIntent::SmartStop;
        }

        // ------------------------------------------------------------------
        // 8. Default
        // ------------------------------------------------------------------
        self.shared.set_preheat(false).await;
        self.shared.set_smart_stop(false).await;
        DecisionIntent::Idle
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

    use crate::event_detector::UseEvent;
    use crate::models::EngineConfig;
    use crate::pattern_store::PatternStore;
    use crate::shared_state::SharedState;

    fn test_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 28, 7, 0, 0).unwrap()
    }

    fn make_state(tank_temp_c: f32) -> GeyserState {
        GeyserState {
            timestamp: test_now(),
            tank_temp_c,
            heating_active: false,
            element_kw: 3.0,
            tank_volume_l: 150.0,
            collector_temp_c: None,
            pump_active: None,
        }
    }

    fn make_engine(pattern_probs: &[(u32, f32)]) -> (DecisionEngine, SharedState) {
        use crate::heat_calc::heat_lead_time_minutes;
        use crate::system::HeatingSystem;

        let config = EngineConfig::default();
        let mut store = PatternStore::new(config.decay_factor);
        let now = test_now();

        // Compute the exact look-ahead the engine will use for a cold 40°C tank.
        let cold_state = make_state(40.0);
        let lead =
            heat_lead_time_minutes(&cold_state, config.setpoint_c, &HeatingSystem::ElectricOnly);
        let base_offset_min: u32 = lead + config.safety_margin_min;

        for &(hour_offset, prob) in pattern_probs {
            let event_time =
                now + Duration::minutes(i64::from(base_offset_min) + i64::from(hour_offset) * 60);
            let confidence = prob;
            // Record 7 times with confidence=1.0 to saturate the bucket to 1.0
            // probability, or 0 times (skip) to leave at 0.0.
            if confidence > 0.0 {
                for _ in 0..7 {
                    store.record_event(&UseEvent {
                        started_at: event_time,
                        ended_at: event_time + Duration::minutes(10),
                        temp_drop_c: 5.0,
                        estimated_volume_l: 30.0,
                        confidence: 1.0,
                    });
                }
            }
        }

        let shared = SharedState::new();
        let engine = DecisionEngine::new(config, store, shared.clone());
        (engine, shared)
    }

    // -----------------------------------------------------------------------
    // Test 1: cold tank + high probability → Preheat
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn cold_tank_high_probability_preheat() {
        let (mut engine, shared) = make_engine(&[(0, 1.0)]);
        let state = make_state(40.0);
        let now = test_now();

        // Pre-seed last_high_temp_event so legionella doesn't fire.
        shared.write().await.last_high_temp_event = Some(now);

        let intent = engine.tick(&state, now).await;

        assert_eq!(intent, DecisionIntent::Preheat { until_temp_c: 60.0 });
        assert_eq!(shared.read().await.preheat_active, true);
    }

    // -----------------------------------------------------------------------
    // Test 2: hot tank + low probability → SmartStop
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn hot_tank_low_probability_smart_stop() {
        let (mut engine, shared) = make_engine(&[]);
        let state = make_state(60.0);
        let now = test_now();

        // Pre-seed last_high_temp_event so legionella doesn't fire.
        shared.write().await.last_high_temp_event = Some(now);

        let intent = engine.tick(&state, now).await;

        assert_eq!(intent, DecisionIntent::SmartStop);
    }

    // -----------------------------------------------------------------------
    // Test 3: cold tank + low probability → Idle
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn cold_tank_low_probability_idle() {
        let (mut engine, shared) = make_engine(&[]);
        let state = make_state(40.0);
        let now = test_now();

        // Pre-seed last_high_temp_event so legionella doesn't fire.
        shared.write().await.last_high_temp_event = Some(now);

        let intent = engine.tick(&state, now).await;

        assert_eq!(intent, DecisionIntent::Idle);
    }

    // -----------------------------------------------------------------------
    // Test 4: boost active → Boost regardless
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn boost_active_returns_boost() {
        let (mut engine, shared) = make_engine(&[(0, 1.0)]);
        let state = make_state(40.0);
        let now = test_now();
        let boost_until = now + Duration::hours(1);

        shared.set_boost_until(Some(boost_until)).await;

        let intent = engine.tick(&state, now).await;

        assert!(matches!(intent, DecisionIntent::Boost { .. }));
    }

    // -----------------------------------------------------------------------
    // Test 5: legionella force-heat after 8 days
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn legionella_force_heat_after_8_days() {
        let (mut engine, shared) = make_engine(&[]);
        let state = make_state(40.0);
        let now = test_now();

        shared.write().await.last_high_temp_event = Some(now - Duration::days(8));

        let intent = engine.tick(&state, now).await;

        assert_eq!(intent, DecisionIntent::Preheat { until_temp_c: 65.0 });
    }
}
