//! Integration smoke test: 14-day synthetic timeline → decision engine.
//!
//! Simulates two weeks of daily morning + evening showers, feeds the events
//! through `EventDetector` → `PatternStore` → `DecisionEngine`, and asserts
//! that by day 7 the engine correctly pre-heats before showers and smart-stops
//! after the last daily use.

use chrono::{DateTime, Duration, TimeZone, Utc};
use smart_geyser_core::{
    decision_engine::{DecisionEngine, DecisionIntent},
    event_detector::{EventDetector, EventDetectorConfig},
    models::{EngineConfig, GeyserState},
    pattern_store::PatternStore,
    shared_state::SharedState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(temp_c: f32, ts: DateTime<Utc>, heating_active: bool) -> GeyserState {
    GeyserState {
        timestamp: ts,
        tank_temp_c: temp_c,
        collector_temp_c: None,
        pump_active: None,
        heating_active,
        element_kw: 3.0,
        tank_volume_l: 150.0,
    }
}

/// Simulate a shower: 8 samples at 1-min intervals, temp drops by `drop_c`
/// total, then 3 stable samples at the final temp. Returns detected events.
fn simulate_shower(
    detector: &mut EventDetector,
    store: &mut PatternStore,
    start: DateTime<Utc>,
    start_temp: f32,
    drop_c: f32,
) {
    // 8 dropping samples
    for i in 0..8i64 {
        let ts = start + Duration::seconds(i * 60);
        let temp = start_temp - (i as f32 / 7.0) * drop_c;
        let state = make_state(temp, ts, false);
        if let Some(event) = detector.feed(state) {
            store.record_event(&event);
        }
    }
    // 3 recovery / stable samples
    let final_temp = start_temp - drop_c;
    for j in 0..3i64 {
        let ts = start + Duration::seconds((8 + j) * 60);
        let state = make_state(final_temp, ts, false);
        if let Some(event) = detector.feed(state) {
            store.record_event(&event);
        }
    }
}

/// Apply standing loss over `hours` at 0.1°C/hr via two samples (start + end).
fn apply_standing_loss(
    detector: &mut EventDetector,
    start: DateTime<Utc>,
    start_temp: f32,
    hours: f32,
) {
    // Gap would exceed max_sample_gap_seconds (120 s) and reset the buffer,
    // which is fine — we only care about the shower events.
    let end_ts = start + Duration::seconds((hours * 3600.0) as i64);
    let end_temp = start_temp - hours * 0.1;
    // Feed end sample; the gap will flush the detector buffer (expected).
    let _ = detector.feed(make_state(end_temp, end_ts, false));
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Build the pattern store by simulating 14 days of showers, then verify
/// that by day 8 the decision engine produces Preheat before the morning
/// shower and SmartStop in the late evening.
#[tokio::test]
async fn decision_engine_learns_and_schedules_correctly() {
    // 2026-01-05 is a Monday — use as day 1.
    let day0 = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();

    let config = EngineConfig::default();
    let mut store = PatternStore::new(config.decay_factor);
    let mut detector = EventDetector::new(EventDetectorConfig::default());

    // --- Learning phase: 14 days of morning (07:00) + evening (19:00) showers ---
    let mut daily_temp = 60.0_f32;

    for day in 0..14i64 {
        let date = day0 + Duration::days(day);

        // Morning shower at 07:00 — 6°C drop (realistic draw from a full tank)
        let morning = date + Duration::hours(7);
        simulate_shower(&mut detector, &mut store, morning, daily_temp, 6.0);

        // The element heats back up by evening (simplified: just reset temp)
        daily_temp = 60.0;

        // Evening shower at 19:00 — 5°C drop
        let evening = date + Duration::hours(19);
        simulate_shower(&mut detector, &mut store, evening, daily_temp, 5.0);
        daily_temp -= 5.0;

        // Overnight standing loss: 12 hours * 0.1°C/hr = 1.2°C
        let midnight = date + Duration::hours(24);
        apply_standing_loss(
            &mut detector,
            evening + Duration::hours(1),
            daily_temp,
            11.0,
        );

        // Apply daily decay.
        store.apply_daily_decay(midnight.date_naive());
        daily_temp = 60.0; // assume element reheat overnight
    }

    // --- Decision phase: day 8 (2026-01-13) ---
    let day8 = day0 + Duration::days(8);
    let shared = SharedState::new();

    // Seed last_high_temp_event to suppress legionella (not the focus here).
    shared.write().await.last_high_temp_event = Some(day8);

    let mut engine = DecisionEngine::new(config, store, shared.clone());

    // Simulate a cold-ish tank before the morning shower (overnight loss).
    let pre_shower_temp = 55.0_f32;

    // 6.2 — Pre-heat should fire before the 07:00 shower.
    // Lead time for 55→60°C, 3 kW, 150 L ≈ 18 min + 20 safety = 38 min.
    // Ticking at 06:30 → look_ahead = 07:08, which lands in the hour-7 bucket
    // where morning showers were recorded (probability = 1.0 ≥ threshold 0.40).
    let morning_tick = day8 + Duration::minutes(6 * 60 + 30); // 06:30
    let morning_state = make_state(pre_shower_temp, morning_tick, false);
    let intent = engine.tick(&morning_state, morning_tick).await;

    assert!(
        matches!(intent, DecisionIntent::Preheat { .. }),
        "expected Preheat before morning shower, got {intent:?}"
    );

    // 6.3 — SmartStop should fire in the late evening (21:00) when no more
    // use is expected until the next day.
    // Tank is hot (element ran); probability of use at 21:30 buffer = 0 (no
    // late-night events recorded).
    let evening_tick = day8 + Duration::hours(21);
    let hot_state = make_state(60.0, evening_tick, false);

    // Re-seed to suppress legionella again (it checks current time vs. last event).
    shared.write().await.last_high_temp_event = Some(evening_tick);

    let evening_intent = engine.tick(&hot_state, evening_tick).await;

    assert!(
        matches!(evening_intent, DecisionIntent::SmartStop),
        "expected SmartStop in late evening, got {evening_intent:?}"
    );
}
