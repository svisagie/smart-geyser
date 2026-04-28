//! Time-of-day usage histogram — the learning brain's memory.

use std::fs::File;
use std::io::{BufReader, BufWriter};

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::event_detector::UseEvent;

// ---------------------------------------------------------------------------
// Bucket index helpers
// ---------------------------------------------------------------------------

/// Map a UTC timestamp to a histogram bucket index (0..168).
///
/// Index = `weekday_index * 24 + hour_of_day`, where `weekday_index` follows
/// `chrono::Weekday::num_days_from_monday()` (Mon=0 … Sun=6).
fn bucket_index(when: DateTime<Utc>) -> usize {
    let weekday = when.weekday().num_days_from_monday() as usize; // 0=Mon … 6=Sun
    let hour = when.hour() as usize;
    weekday * 24 + hour
}

// ---------------------------------------------------------------------------
// PatternStore
// ---------------------------------------------------------------------------

/// A 168-bucket (7 days × 24 hours) exponentially-decaying usage histogram.
///
/// Each bucket accumulates a weight whenever a hot-water-use event is
/// recorded. `probability_at` normalises to [0.0, 1.0] by dividing by the
/// maximum bucket value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternStore {
    buckets: Vec<f32>,
    decay_factor: f32,
    last_decay_applied: Option<NaiveDate>,
}

impl PatternStore {
    /// Create a new, all-zero store with the given per-day decay factor.
    #[must_use]
    pub fn new(decay_factor: f32) -> Self {
        Self {
            buckets: vec![0.0_f32; 168],
            decay_factor,
            last_decay_applied: None,
        }
    }

    /// Record a hot-water use event.
    ///
    /// Increments the bucket for `event.started_at` by `event.confidence`,
    /// and smears ±1 hour (adjacent buckets) by `event.confidence * 0.3`.
    /// All bucket values are clamped to [0.0, 1000.0].
    pub fn record_event(&mut self, event: &UseEvent) {
        let centre = bucket_index(event.started_at);
        let confidence = event.confidence;

        // Centre bucket.
        self.add_to_bucket(centre, confidence);

        // Adjacent buckets (wrap around within the 168-bucket ring).
        let prev = (centre + 168 - 1) % 168;
        let next = (centre + 1) % 168;
        self.add_to_bucket(prev, confidence * 0.3);
        self.add_to_bucket(next, confidence * 0.3);
    }

    fn add_to_bucket(&mut self, idx: usize, amount: f32) {
        self.buckets[idx] = (self.buckets[idx] + amount).clamp(0.0, 1000.0);
    }

    /// Apply exponential decay.
    ///
    /// If `last_decay_applied` is `None` or `today > last_decay_applied`,
    /// multiplies every bucket by `decay_factor ^ days_since_last_decay` and
    /// records `today` as the new reference date.
    pub fn apply_daily_decay(&mut self, today: NaiveDate) {
        let days = match self.last_decay_applied {
            None => 1i64,
            Some(last) => {
                let delta = (today - last).num_days();
                if delta <= 0 {
                    return; // already decayed for today (or future date)
                }
                delta
            }
        };

        #[allow(clippy::cast_possible_truncation)]
        let factor = self.decay_factor.powi(days as i32);
        for b in &mut self.buckets {
            *b *= factor;
        }
        self.last_decay_applied = Some(today);
    }

    /// Return the normalised probability [0.0, 1.0] for the hour at `when`.
    ///
    /// Normalises by the maximum bucket value. Returns 0.0 if all buckets
    /// are zero.
    #[must_use]
    pub fn probability_at(&self, when: DateTime<Utc>) -> f32 {
        let max = self.buckets.iter().copied().fold(0.0_f32, f32::max);
        if max == 0.0 {
            return 0.0;
        }
        let idx = bucket_index(when);
        self.buckets[idx] / max
    }

    /// Scan forward up to 168 hours from `after` and return the first hour
    /// whose probability meets or exceeds `threshold`.
    ///
    /// Returns `None` if no such window exists within 168 hours.
    #[must_use]
    pub fn next_high_probability_window(
        &self,
        after: DateTime<Utc>,
        threshold: f32,
    ) -> Option<DateTime<Utc>> {
        use chrono::Duration;

        // Start from the next whole hour after `after`.
        let start = {
            let truncated = after
                .with_minute(0)
                .and_then(|t| t.with_second(0))
                .and_then(|t| t.with_nanosecond(0))
                .unwrap_or(after);
            // If `after` is already on the hour boundary, still advance one
            // hour so we start *after* the given time.
            truncated + Duration::hours(1)
        };

        for h in 0..168_i64 {
            let candidate = start + Duration::hours(h);
            if self.probability_at(candidate) >= threshold {
                return Some(candidate);
            }
        }
        None
    }

    /// Serialise to pretty JSON and write to `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or serialisation fails.
    pub fn save_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    /// Read `path` and deserialise from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or deserialisation fails.
    pub fn load_from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let store = serde_json::from_reader(reader)?;
        Ok(store)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_detector::UseEvent;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    /// Build a `UseEvent` at `2026-01-05 + weekday days` at `hour:00:00 UTC`.
    /// 2026-01-05 is a Monday (num_days_from_monday() == 0).
    fn make_event(weekday: u32, hour: u32, confidence: f32) -> UseEvent {
        // 2026-01-05 is Monday; add weekday days to reach the desired weekday.
        let base = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let started_at =
            base + chrono::Duration::days(weekday as i64) + chrono::Duration::hours(hour as i64);
        let ended_at = started_at + chrono::Duration::minutes(10);
        UseEvent {
            started_at,
            ended_at,
            temp_drop_c: 5.0,
            estimated_volume_l: 30.0,
            confidence,
        }
    }

    // -----------------------------------------------------------------------
    // Test 1 — morning shower bucket becomes highest
    // -----------------------------------------------------------------------
    #[test]
    fn test_morning_shower_bucket_is_highest() {
        let mut store = PatternStore::new(0.995);

        // Record Monday 7am seven times.
        for _ in 0..7 {
            store.record_event(&make_event(0, 7, 1.0));
        }

        let monday_7am = Utc.with_ymd_and_hms(2026, 1, 5, 7, 0, 0).unwrap();
        let prob = store.probability_at(monday_7am);
        // The Monday 7am bucket is the max, so normalised probability == 1.0.
        assert_eq!(prob, 1.0_f32);

        // Wednesday 14:00 has never been recorded → 0.0.
        let wednesday_14 = Utc.with_ymd_and_hms(2026, 1, 7, 14, 0, 0).unwrap();
        assert_eq!(store.probability_at(wednesday_14), 0.0_f32);
    }

    // -----------------------------------------------------------------------
    // Test 2 — decay reduces probability toward zero
    // -----------------------------------------------------------------------
    #[test]
    fn test_decay_reduces_to_near_zero() {
        let mut store = PatternStore::new(0.995);
        store.record_event(&make_event(0, 7, 1.0));

        let start = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        for i in 0..1500_i64 {
            store.apply_daily_decay(start + chrono::Duration::days(i));
        }

        for &b in store.buckets.iter() {
            assert!(b < 0.001, "bucket should be near zero but was {b}");
        }
    }

    // -----------------------------------------------------------------------
    // Test 3 — round-trip save / load
    // -----------------------------------------------------------------------
    #[test]
    fn test_round_trip_save_load() {
        let mut store = PatternStore::new(0.995);
        store.record_event(&make_event(0, 7, 1.0));
        store.record_event(&make_event(2, 14, 0.8));
        store.record_event(&make_event(5, 9, 0.5));

        let path = std::env::temp_dir().join("pattern_store_test.json");
        store.save_to_path(&path).expect("save failed");

        let loaded = PatternStore::load_from_path(&path).expect("load failed");

        for (i, (&orig, &loaded_val)) in store.buckets.iter().zip(loaded.buckets.iter()).enumerate()
        {
            assert!(
                (orig - loaded_val).abs() < 0.001,
                "bucket {i} mismatch: orig={orig} loaded={loaded_val}"
            );
        }

        // Clean up.
        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Test 4 — next_high_probability_window finds the window
    // -----------------------------------------------------------------------
    #[test]
    fn test_next_high_probability_window_finds_window() {
        let mut store = PatternStore::new(0.995);

        // Record Monday 8am seven times.
        for _ in 0..7 {
            store.record_event(&make_event(0, 8, 1.0));
        }

        // Search from Monday 06:00Z; the first hit should be Monday 7am
        // (smear bucket) or Monday 8am (centre).
        let after = Utc.with_ymd_and_hms(2026, 1, 5, 6, 0, 0).unwrap();
        let result = store.next_high_probability_window(after, 0.5);

        assert!(result.is_some(), "expected Some window, got None");
        let window = result.unwrap();
        // Result must be on Monday (weekday 0 from Mon) and hour 7 or 8
        // (smear or centre bucket).
        assert!(
            window.hour() == 7 || window.hour() == 8,
            "expected hour 7 or 8, got {}",
            window.hour()
        );
        assert_eq!(
            window.weekday().num_days_from_monday(),
            0,
            "expected Monday"
        );
    }

    // -----------------------------------------------------------------------
    // Test 5 — adjacent bucket smearing
    // -----------------------------------------------------------------------
    #[test]
    fn test_adjacent_bucket_smearing() {
        let mut store = PatternStore::new(0.995);
        store.record_event(&make_event(0, 7, 1.0)); // Monday 7am

        let monday_8am = Utc.with_ymd_and_hms(2026, 1, 5, 8, 0, 0).unwrap();
        let monday_6am = Utc.with_ymd_and_hms(2026, 1, 5, 6, 0, 0).unwrap();

        assert!(
            store.probability_at(monday_8am) > 0.0,
            "Monday 8am should have smear weight"
        );
        assert!(
            store.probability_at(monday_6am) > 0.0,
            "Monday 6am should have smear weight"
        );
    }
}
