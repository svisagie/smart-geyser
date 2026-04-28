//! Hot-water use event detector for the smart-geyser decision engine.
//!
//! Implements a falling-edge state machine over `GeyserState` temperature
//! samples to detect when hot water has been drawn from the tank.  Detected
//! events feed the time-of-day histogram used by the decision engine for
//! pre-heat scheduling.
//!
//! Spec reference: §13 Phase 2 — Event Detection.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::GeyserState;

// ---------------------------------------------------------------------------
// UseEvent
// ---------------------------------------------------------------------------

/// A detected hot-water draw event.
///
/// Emitted by [`EventDetector::feed`] when a confirmed temperature-drop
/// pattern is observed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseEvent {
    /// Timestamp of the first sample that exceeded the drop threshold.
    pub started_at: DateTime<Utc>,
    /// Timestamp of the sample at which the temperature stabilised.
    pub ended_at: DateTime<Utc>,
    /// Total temperature drop over the event, in °C.
    pub temp_drop_c: f32,
    /// Rough estimate of water volume drawn, in litres.
    ///
    /// Computed as `temp_drop_c * tank_volume_l_at_start * 0.7 / 40.0`,
    /// where 0.7 corrects for mixing effects and 40.0 normalises against a
    /// typical 40 °C setpoint-to-cold differential.
    pub estimated_volume_l: f32,
    /// Confidence score in the range 0.0–1.0.
    pub confidence: f32,
}

// ---------------------------------------------------------------------------
// EventDetectorConfig
// ---------------------------------------------------------------------------

/// Tunables for the falling-edge event detector.
///
/// All defaults match the values documented in the spec (§13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDetectorConfig {
    /// Rate of drop (°C/min) above which we suspect water use. Default: 0.5.
    pub drop_threshold_c_per_min: f32,
    /// Total drop (°C) required to confirm a use event. Default: 3.0.
    pub min_drop_c: f32,
    /// After an event ends, ignore drops for this many seconds. Default: 300.
    pub debounce_seconds: u32,
    /// Drop rate (°C/min) below which the temperature is considered stable.
    /// Default: 0.1.
    pub idle_recovery_threshold_c_per_min: f32,
    /// Maximum seconds between consecutive samples before treating as a
    /// dropout. Default: 120.
    pub max_sample_gap_seconds: u32,
}

impl Default for EventDetectorConfig {
    fn default() -> Self {
        Self {
            drop_threshold_c_per_min: 0.5,
            min_drop_c: 3.0,
            debounce_seconds: 300,
            idle_recovery_threshold_c_per_min: 0.1,
            max_sample_gap_seconds: 120,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// Tracks the start of a candidate drop event.
struct DropCandidate {
    started_at: DateTime<Utc>,
    start_temp_c: f32,
    tank_volume_l: f32,
}

// ---------------------------------------------------------------------------
// EventDetector
// ---------------------------------------------------------------------------

/// Stateful falling-edge detector over a stream of [`GeyserState`] samples.
///
/// Feed samples in chronological order via [`EventDetector::feed`].  The
/// detector maintains a ring buffer of recent samples and emits a
/// [`UseEvent`] whenever it confirms a temperature drop consistent with a
/// hot-water draw.
pub struct EventDetector {
    config: EventDetectorConfig,
    /// Ring buffer of recent samples, newest last.
    samples: VecDeque<GeyserState>,
    /// Maximum samples to retain (covers ~1 hour at 1-sample/min).
    max_samples: usize,
    /// Timestamp of the last emitted event (for debouncing).
    last_event_at: Option<DateTime<Utc>>,
    /// State of the current falling-edge candidate.
    candidate: Option<DropCandidate>,
}

impl EventDetector {
    /// Create a new detector with the given configuration.
    ///
    /// The ring buffer is pre-sized to hold 60 samples (1 h at 1-sample/min).
    #[must_use]
    pub fn new(config: EventDetectorConfig) -> Self {
        Self {
            config,
            samples: VecDeque::new(),
            max_samples: 60,
            last_event_at: None,
            candidate: None,
        }
    }

    /// Feed the next [`GeyserState`] sample into the detector.
    ///
    /// Returns `Some(UseEvent)` when a confirmed hot-water-draw is detected,
    /// otherwise `None`.
    ///
    /// # Panics
    ///
    /// Does not panic — the `unwrap()` inside is guarded by the `is_some()`
    /// check in the candidate-resolution branch.
    #[must_use]
    pub fn feed(&mut self, state: GeyserState) -> Option<UseEvent> {
        // ------------------------------------------------------------------
        // 1. Stale / dropout detection
        // ------------------------------------------------------------------
        if let Some(last) = self.samples.back() {
            let gap_secs = (state.timestamp - last.timestamp).num_seconds();

            // Reject samples that are not strictly after the previous one, or
            // that arrive after a gap large enough to break continuity.
            if gap_secs <= 0 || gap_secs > i64::from(self.config.max_sample_gap_seconds) {
                self.samples.clear();
                self.candidate = None;
                self.samples.push_back(state);
                return None;
            }
        }

        // ------------------------------------------------------------------
        // 2. Push sample; evict oldest if over capacity
        // ------------------------------------------------------------------
        self.samples.push_back(state.clone());
        if self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }

        // ------------------------------------------------------------------
        // 3. Need at least 2 samples to compute a rate
        // ------------------------------------------------------------------
        if self.samples.len() < 2 {
            return None;
        }

        let n = self.samples.len();
        let prev = &self.samples[n - 2];
        let current = &self.samples[n - 1];

        // ------------------------------------------------------------------
        // 4. Compute drop rate
        // ------------------------------------------------------------------
        #[allow(clippy::cast_precision_loss)]
        let elapsed_minutes = (current.timestamp - prev.timestamp).num_seconds() as f32 / 60.0;

        if elapsed_minutes <= 0.0 {
            return None;
        }

        let drop_rate_c_per_min = (prev.tank_temp_c - current.tank_temp_c) / elapsed_minutes;

        // ------------------------------------------------------------------
        // 5. False-positive guard: heating active → clear candidate, bail
        // ------------------------------------------------------------------
        if state.heating_active {
            self.candidate = None;
            return None;
        }

        // ------------------------------------------------------------------
        // 6. Candidate start
        // ------------------------------------------------------------------
        if drop_rate_c_per_min >= self.config.drop_threshold_c_per_min && self.candidate.is_none() {
            // Debounce check
            if let Some(last_at) = self.last_event_at {
                let secs_since = (state.timestamp - last_at).num_seconds();
                if secs_since < i64::from(self.config.debounce_seconds) {
                    return None;
                }
            }

            self.candidate = Some(DropCandidate {
                started_at: state.timestamp,
                start_temp_c: prev.tank_temp_c,
                tank_volume_l: state.tank_volume_l,
            });
        }

        // ------------------------------------------------------------------
        // 7. Candidate resolution
        // ------------------------------------------------------------------
        if self.candidate.is_some()
            && drop_rate_c_per_min < self.config.idle_recovery_threshold_c_per_min
        {
            let cand = self.candidate.take().unwrap();
            let total_drop = cand.start_temp_c - state.tank_temp_c;

            if total_drop >= self.config.min_drop_c {
                let confidence = (total_drop / self.config.min_drop_c).min(1.0);

                let event = UseEvent {
                    started_at: cand.started_at,
                    ended_at: state.timestamp,
                    temp_drop_c: total_drop,
                    estimated_volume_l: total_drop * cand.tank_volume_l * 0.7 / 40.0,
                    confidence,
                };

                self.last_event_at = Some(state.timestamp);
                return Some(event);
            }

            // Drop was too small — discard candidate silently.
            return None;
        }

        // ------------------------------------------------------------------
        // 8. Default
        // ------------------------------------------------------------------
        None
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

    /// Fixed epoch used by all test helpers: 2026-01-01T06:00:00Z.
    fn epoch() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 6, 0, 0).unwrap()
    }

    /// Build a minimal `GeyserState` with the given temperature, a timestamp
    /// offset in seconds from the test epoch, and the given `heating_active`
    /// flag.  All optional fields are left as `None`; element power and volume
    /// are fixed at 3.0 kW and 150 L.
    fn make_state(temp_c: f32, seconds_from_start: i64, heating_active: bool) -> GeyserState {
        GeyserState {
            timestamp: epoch() + chrono::Duration::seconds(seconds_from_start),
            tank_temp_c: temp_c,
            collector_temp_c: None,
            pump_active: None,
            heating_active,
            element_kw: 3.0,
            tank_volume_l: 150.0,
        }
    }

    // -----------------------------------------------------------------------
    // Test 1: clean shower event → one event
    // -----------------------------------------------------------------------
    #[test]
    fn clean_shower_emits_one_event() {
        let mut det = EventDetector::new(EventDetectorConfig::default());

        // 8 samples at 1-min intervals, temp drops 55.0 → 50.0 (−0.625 °C/min).
        let temps: Vec<f32> = (0..=7).map(|i| 55.0 - i as f32 * (5.0 / 7.0)).collect();

        let mut events: Vec<UseEvent> = Vec::new();

        for (i, &temp) in temps.iter().enumerate() {
            let secs = i as i64 * 60;
            if let Some(e) = det.feed(make_state(temp, secs, false)) {
                events.push(e);
            }
        }

        // 2 recovery samples — temperature flat at 50.0
        for j in 0..2_i64 {
            let secs = 7 * 60 + (j + 1) * 60;
            if let Some(e) = det.feed(make_state(50.0, secs, false)) {
                events.push(e);
            }
        }

        assert_eq!(events.len(), 1, "expected exactly one event");
        let ev = &events[0];
        assert!(
            ev.temp_drop_c >= 4.5 && ev.temp_drop_c <= 5.5,
            "temp_drop_c {} not in [4.5, 5.5]",
            ev.temp_drop_c
        );
        assert!(ev.confidence > 0.0, "confidence should be positive");
    }

    // -----------------------------------------------------------------------
    // Test 2: heating-then-cooling → no event
    // -----------------------------------------------------------------------
    #[test]
    fn heating_active_suppresses_events() {
        let mut det = EventDetector::new(EventDetectorConfig::default());

        // Simulate a large, fast drop while heating is active — should never
        // produce an event.
        let temps = [60.0_f32, 55.0, 50.0, 45.0, 40.0, 38.0, 37.9];
        for (i, &temp) in temps.iter().enumerate() {
            let result = det.feed(make_state(temp, i as i64 * 60, true));
            assert!(result.is_none(), "expected None at sample {i} but got Some");
        }
    }

    // -----------------------------------------------------------------------
    // Test 3: sensor dropout → no event emitted during gap
    // -----------------------------------------------------------------------
    #[test]
    fn stale_sample_clears_buffer_and_returns_none() {
        let mut det = EventDetector::new(EventDetectorConfig::default());

        // 3 normal samples
        let _ = det.feed(make_state(55.0, 0, false));
        let _ = det.feed(make_state(54.5, 60, false));
        let _ = det.feed(make_state(54.0, 120, false));

        // A sample with the same timestamp as the previous (stale) — must
        // return None and clear the buffer.
        let stale = make_state(53.5, 120, false); // duplicate timestamp
        let result = det.feed(stale);

        assert!(result.is_none(), "stale sample should return None");
        // After the stale sample the ring buffer should contain only the
        // stale sample itself (length == 1 → no rate can be computed).
        assert_eq!(
            det.samples.len(),
            1,
            "ring buffer should be cleared to just the stale sample"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: two events within debounce → only one event
    // -----------------------------------------------------------------------
    #[test]
    fn second_event_within_debounce_suppressed() {
        let mut det = EventDetector::new(EventDetectorConfig::default());

        // --- First shower sequence (t=0..9 min) ----------------------------
        // 8 dropping samples then 1 recovery.
        let mut events: Vec<UseEvent> = Vec::new();

        for i in 0..8_i64 {
            let temp = 55.0 - i as f32 * (5.0 / 7.0);
            if let Some(e) = det.feed(make_state(temp, i * 60, false)) {
                events.push(e);
            }
        }
        // Recovery sample to trigger resolution
        if let Some(e) = det.feed(make_state(50.0, 8 * 60, false)) {
            events.push(e);
        }
        if let Some(e) = det.feed(make_state(50.0, 9 * 60, false)) {
            events.push(e);
        }

        assert_eq!(
            events.len(),
            1,
            "first shower should produce exactly one event"
        );

        // --- Second shower sequence immediately after (t=9..17 min) --------
        // Starts at t=9*60 which is well within the 300-second debounce from
        // when the first event was emitted.
        let base = 9 * 60_i64;
        for i in 0..8_i64 {
            let temp = 50.0 - i as f32 * (5.0 / 7.0);
            if let Some(e) = det.feed(make_state(temp, base + i * 60, false)) {
                events.push(e);
            }
        }
        if let Some(e) = det.feed(make_state(45.0, base + 8 * 60, false)) {
            events.push(e);
        }
        if let Some(e) = det.feed(make_state(45.0, base + 9 * 60, false)) {
            events.push(e);
        }

        assert_eq!(
            events.len(),
            1,
            "second sequence within debounce window should not produce an event"
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: slow standing-loss → no event
    // -----------------------------------------------------------------------
    #[test]
    fn slow_standing_loss_produces_no_event() {
        let mut det = EventDetector::new(EventDetectorConfig::default());

        // 20 samples, each 10 minutes apart, dropping 0.05 °C per interval
        // → rate ≈ 0.005 °C/min, far below the 0.5 °C/min threshold.
        for i in 0..20_i64 {
            let temp = 60.0 - i as f32 * 0.05;
            let secs = i * 600; // 10-minute intervals
            let result = det.feed(make_state(temp, secs, false));
            assert!(
                result.is_none(),
                "standing loss at sample {i} should not emit an event"
            );
        }
    }
}
