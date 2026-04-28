# Phase 2 — Event Detection & Pattern Store

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 2, §4 (SharedEngineState), §3.6 (smart-stop semantics)

**Goal:** Build the learning brain. Detect hot-water-use events from temperature drops, accumulate them in a time-of-day histogram, and produce pre-heat decisions with smart-stop awareness — but **without** the PV opportunity logic (that lands in Phase 3).

**Exit criteria:** `cargo test -p smart-geyser-core` still green. Synthetic temperature traces fed into the event detector produce the expected events; the pattern store predicts upcoming use windows; the decision engine emits sensible Preheat / Idle / SmartStop intents.

---

## 1. Event detection (`event_detector.rs`)

- [x] 1.1 Define `UseEvent` struct (started_at, ended_at, temp_drop_c, estimated_volume_l, confidence)
- [x] 1.2 Define `EventDetectorConfig` (drop_threshold_c_per_min, min_drop_c, debounce_seconds, idle_recovery_threshold)
- [x] 1.3 Implement `EventDetector` with a ring buffer of recent `GeyserState` samples
- [x] 1.4 `feed(&mut self, state: GeyserState) -> Option<UseEvent>` — emits an event when a falling-edge pattern resolves
- [x] 1.5 Reject false positives: sample noise, heating cycles in reverse, sensor dropouts
- [x] 1.6 Unit tests with synthetic traces:
  - Clean shower event (5°C drop over 8 min) → one event
  - Heating-then-cooling cycle → no event
  - Sensor dropout (stale timestamps) → no event
  - Two events within debounce window → one event
  - Slow standing-loss decay (0.1°C/hr) → no event

## 2. Pattern store (`pattern_store.rs`)

- [x] 2.1 Define `PatternStore` with a 24h × 7d histogram (168 buckets, 1h resolution to start — refine later if needed)
- [x] 2.2 `record_event(&mut self, event: &UseEvent)` — increments the appropriate bucket(s) with a weighting
- [x] 2.3 Apply daily decay: `decay_factor` from `EngineConfig` (default 0.995) applied per day to all buckets
- [x] 2.4 `probability_at(&self, when: DateTime<Utc>) -> f32` returns 0.0–1.0
- [x] 2.5 `next_high_probability_window(&self, after: DateTime<Utc>, threshold: f32) -> Option<DateTime<Utc>>`
- [x] 2.6 Persistence: serialise to JSON via serde, plus `save_to_path` / `load_from_path` helpers
- [x] 2.7 Unit tests:
  - Single morning shower recorded daily for 7 days → that bucket has highest probability
  - Decay reduces probability over time toward zero with no reinforcement
  - Round-trip save/load preserves exact state

## 3. Shared engine state (`shared_state.rs` — new file, called out in spec §4)

- [x] 3.1 Define `SharedEngineState` per spec §4 (smart_stop_active, opportunity_active, last_opportunity_start, preheat_active, boost_until)
- [x] 3.2 Wrap in `Arc<RwLock<_>>` access pattern; provide `SharedState` newtype with ergonomic helpers (`is_boosting()`, `set_preheat(bool)`, etc.)
- [x] 3.3 Unit tests for concurrent read/write semantics with `tokio::test`

## 4. Decision engine skeleton (`decision_engine.rs`)

- [x] 4.1 Define `DecisionIntent` enum: `Idle`, `Preheat { until_temp_c }`, `Boost { until }`, `SmartStop` (no Opportunity yet — Phase 3)
- [x] 4.2 Implement `DecisionEngine` with refs to `PatternStore`, `EngineConfig`, `SharedState`
- [x] 4.3 `tick(&mut self, state: &GeyserState, now: DateTime<Utc>) -> DecisionIntent`
- [x] 4.4 Pre-heat trigger: `pattern_store.probability_at(now + lead_time) >= preheat_threshold` AND tank below `setpoint_c - hysteresis_c`
- [x] 4.5 Smart-stop trigger: probability of use in next `cutoff_buffer_min` minutes < `late_use_threshold` AND tank above `setpoint_c - hysteresis_c`
- [x] 4.6 Boost passthrough: if `shared_state.boost_until > now`, return `Boost`
- [x] 4.7 Lead time: use `heat_calc::heat_lead_time_minutes` + `safety_margin_min`
- [x] 4.8 Unit tests with mock provider + fixed clock:
  - Tank cold + high probability in 1h ⇒ `Preheat`
  - Tank hot + low probability rest of day ⇒ `SmartStop`
  - Mid-state, mid-probability ⇒ `Idle`
  - Boost active ⇒ always `Boost` regardless of other state
  - Pre-heat respects lead time: starts exactly `lead_time + safety_margin` before predicted use

## 5. Legionella safety override

- [x] 5.1 Track `last_high_temp_event` (≥ 65°C for ≥ 30 min) in `SharedEngineState` or a sidecar struct
- [x] 5.2 If `now - last_high_temp_event > legionella_interval_days`, force a `Preheat { until_temp_c: 65.0 }` regardless of pattern
- [x] 5.3 Unit test: simulate 8 days of cold tank → on day 7 (default interval) the engine forces a heat cycle

## 6. Integration smoke test

- [x] 6.1 `tests/decision_engine_integration.rs` — feed a 14-day synthetic timeline (morning + evening showers, gradual standing loss, occasional weekends with different patterns)
- [x] 6.2 Assert: by day 7+ the pre-heat intent fires within 10 min of typical shower start
- [x] 6.3 Assert: late-evening smart-stop kicks in once daily-use probability drops

## 7. Phase 2 wrap-up

- [x] 7.1 Inside the dev container: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` all green (per CLAUDE.md Docker-only rule)
- [x] 7.2 Tag commit `phase-2-complete`
- [x] 7.3 Update [CLAUDE.md](../CLAUDE.md) with any architectural decisions or learnings
- [x] 7.4 Open Phase 3 task file when starting that phase
