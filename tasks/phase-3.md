# Phase 3 — Opportunity Engine

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 3, §3 (Opportunity Heating Engine), §4 (SharedEngineState priority order)

**Goal:** Implement PV-surplus opportunity heating. `solar_window.rs` provides sunrise/sunset awareness; `opportunity_engine.rs` evaluates trigger paths A/B/C, enforces suppress conditions, and interacts correctly with smart-stop and solar thermal. The `DecisionIntent` enum gains an `Opportunity` variant and the priority order is enforced.

**PV is optional:** The service holds `Option<Arc<dyn PVSystemProvider>>`. When `None`, the `OpportunityEngine` is never constructed — `opportunity_active` stays `false` permanently and the system behaves as a pure pattern controller. `OpportunityEngine::new` must document this invariant in its doc comment. No code in `decision_engine.rs` or `shared_state.rs` should reference or check for a PV provider.

**Exit criteria:** `cargo test -p smart-geyser-core` passes. Synthetic PV state streams fed into the opportunity engine produce the correct trigger/suppress decisions across all three paths; smart-stop override scenarios are explicitly tested; solar thermal interaction tests are green. No I/O in any of these modules — pure logic only.

---

## 1. Solar window (`solar_window.rs`)

- [ ] 1.1 Implement `SolarWindow::is_active(&self, now: DateTime<Utc>) -> bool` — returns `true` when meaningful PV generation time remains (i.e. `minutes_remaining >= min_remaining_minutes`)
- [ ] 1.2 Implement `SolarWindow::minutes_remaining(&self, now: DateTime<Utc>) -> u32` — compute minutes until solar window closes, clamping to 0 when the window has ended
- [ ] 1.3 Implement the latitude/longitude sunrise/sunset calculation path (use a pure-Rust solar position formula; no network calls, no system calls)
- [ ] 1.4 Implement the fallback path: when latitude/longitude are not configured, default solar window to 07:00–17:00 local time per spec §3.4
- [ ] 1.5 Unit tests for `is_active` and `minutes_remaining`:
  - High noon in mid-summer → active, large minutes remaining
  - 16:20 with `min_remaining_minutes = 45` → not active (< 45 min left in 17:00 fallback window)
  - 16:20 with `min_remaining_minutes = 30` → active (40 min left)
  - Midnight → not active, minutes remaining = 0
  - Fallback path (no lat/lon): 08:00 → active; 18:00 → not active

## 2. `DecisionIntent` update (`decision_engine.rs`)

- [x] 2.1 Add `Opportunity { reason: OpportunityReason, target_temp_c: f32 }` variant to the `DecisionIntent` enum (no Opportunity variant existed in Phase 2 — spec §4 priority order item 3)
- [x] 2.2 Define `OpportunityReason` enum with variants: `BatteryFullExporting` (Path A), `BatteryFullPvCoverage` (Path B), `BatteryFullSocOnly` (Path C)
- [x] 2.3 Update any existing match arms on `DecisionIntent` in `decision_engine.rs` and tests to handle the new variant

## 3. `SharedEngineState` update (`shared_state.rs`)

- [ ] 3.1 Verify `opportunity_active` and `last_opportunity_start` fields are present in `SharedEngineState` per spec §4 (should already exist from Phase 2 §3.1 — confirm or add if missing)
- [ ] 3.2 Add ergonomic helpers to the `SharedState` newtype: `is_opportunity_active()`, `set_opportunity(bool, Option<DateTime<Utc>>)`
- [ ] 3.3 Unit test: concurrent write of `set_opportunity(true, Some(now))` followed by read of `is_opportunity_active()` returns `true`

## 4. Suppress-condition helpers (internal to `opportunity_engine.rs`)

- [ ] 4.1 Implement `tank_too_hot(state: &GeyserState, config: &OpportunityConfig) -> bool` — `true` when `tank_temp_c >= opportunity_max_temp_c`
- [ ] 4.2 Implement `already_heating(state: &GeyserState) -> bool` — `true` when `state.heating_active == true`
- [ ] 4.3 Implement `solar_thermal_doing_useful_work(state: &GeyserState) -> bool` — per spec §3.7: collector hotter than tank by > 5°C AND pump active; returns `false` if either field is `None`
- [ ] 4.4 Implement `solar_thermal_suppress(state: &GeyserState, config: &OpportunityConfig) -> bool` — per spec §3.7: `solar_thermal_doing_useful_work` AND `tank_temp_c > solar_thermal_ceiling_c` (default 65°C); opportunity proceeds in parallel when tank is below the ceiling
- [ ] 4.5 Implement `battery_soc_dropped(pv: &PVSystemState, config: &OpportunityConfig, prior_soc: f32) -> bool` — `true` when SOC has fallen by more than `soc_hysteresis_pct` from the session-start SOC
- [ ] 4.6 Unit tests for each suppress helper in isolation:
  - `tank_too_hot`: temp at exactly `opportunity_max_temp_c` → suppressed; 1°C below → not suppressed
  - `already_heating`: `heating_active = true` → suppressed; `false` → not suppressed
  - `solar_thermal_doing_useful_work`: collector 70°C, tank 55°C, pump active → `true`; pump `None` → `false`; collector < tank + 5.0 → `false`
  - `solar_thermal_suppress`: useful work AND tank 66°C (> 65°C ceiling) → suppressed; tank 64°C → not suppressed (parallel allowed)
  - `battery_soc_dropped`: SOC dropped 4% with hysteresis 3% → `true`; dropped 2% → `false`

## 5. Trigger path evaluation (`opportunity_engine.rs`)

- [ ] 5.1 Implement `evaluate_path_a(pv: &PVSystemState, config: &OpportunityConfig) -> bool` — Path A: `battery_soc_pct >= soc_full_threshold` AND `grid_power_w < -export_floor_w`; returns `false` when `grid_power_w` is `None`
- [ ] 5.2 Implement `evaluate_path_b(pv: &PVSystemState, geyser: &GeyserState, config: &OpportunityConfig) -> bool` — Path B: `battery_soc_pct >= soc_full_threshold` AND `(pv_power_w - load_power_w) >= element_kw * 1000.0 * pv_coverage_ratio`; returns `false` when either `pv_power_w` or `load_power_w` is `None`
- [ ] 5.3 Implement `evaluate_path_c(pv: &PVSystemState, config: &OpportunityConfig, solar_window: &SolarWindow, now: DateTime<Utc>) -> bool` — Path C: `battery_soc_pct >= soc_full_threshold` AND `solar_window.is_active(now)` AND `battery_power_w.abs() < BATTERY_IDLE_THRESHOLD_W` (use 50 W as idle threshold); returns `false` when `battery_power_w` is `None`
- [ ] 5.4 Implement path selection priority: Path A evaluated first (preferred); if `GridPower` capability absent, fall through to Path B; if `PvPower` or `LoadPower` capability absent, fall through to Path C; attach the matching `OpportunityReason` to the result
- [ ] 5.5 Unit tests for each trigger path:
  - Path A fires: SOC 97%, grid −350 W → trigger; SOC 94% → no trigger; grid −100 W (below floor) → no trigger; `grid_power_w = None` → no trigger (falls through)
  - Path B fires: SOC 96%, PV 4000 W, load 800 W, element 3 kW, coverage 0.85 → trigger (surplus 3200 W ≥ 2550 W); PV 3000 W, load 800 W → no trigger (2200 W < 2550 W); either power field `None` → no trigger
  - Path C fires: SOC 96%, solar window active, `battery_power_w = 30 W` → trigger; `battery_power_w = 800 W` (charging) → no trigger; solar window closed → no trigger
  - Path C: `battery_power_w = None` → no trigger

## 6. `OpportunityEngine` main logic (`opportunity_engine.rs`)

- [ ] 6.1 Define `OpportunityEngine` struct holding refs to `OpportunityConfig`, `SolarWindow` (optional), and `SharedState`; track `session_start_soc: Option<f32>` and `session_started_at: Option<DateTime<Utc>>` for anti-cycling
- [ ] 6.2 Implement `tick(&mut self, pv: &PVSystemState, geyser: &GeyserState, now: DateTime<Utc>) -> DecisionIntent` — the main evaluation loop
- [ ] 6.3 Evaluation order inside `tick`:
  1. If any suppress condition fires (spec §3.3) → return `DecisionIntent::Idle` (clear `opportunity_active` in shared state)
  2. Evaluate trigger paths A → B → C; if none fires → return `DecisionIntent::Idle`
  3. If currently in an active session and `min_run_minutes` not elapsed → hold `Opportunity` intent (anti-cycling)
  4. If SOC dropped by `soc_hysteresis_pct` from session-start SOC AND `min_run_minutes` elapsed → stop session, return `Idle`
  5. On new trigger: set `session_started_at`, `session_start_soc`; set `opportunity_active = true`; return `DecisionIntent::Opportunity { reason, target_temp_c: config.opportunity_max_temp_c }`
- [ ] 6.4 Smart-stop override: when `config.override_smart_stop == true`, the `Opportunity` intent is valid even when `smart_stop_active == true` in `SharedEngineState` — document the contract in a comment referencing spec §3.6
- [ ] 6.5 Unit tests for `tick` (fixed clock):
  - All suppress conditions clear + Path A trigger → `DecisionIntent::Opportunity { reason: BatteryFullExporting, .. }`
  - Tank at `opportunity_max_temp_c` → suppress → `Idle`
  - `heating_active = true` → suppress → `Idle`
  - Solar window closed + only Path C available → suppress → `Idle`
  - SOC drops from 97% to 93% (hysteresis 3%) after 20 min (min_run 15 min elapsed) → session ends → `Idle`
  - SOC drops from 97% to 93% after 10 min (min_run 15 min NOT elapsed) → anti-cycling → `Opportunity` maintained

## 7. Smart-stop override integration tests

- [ ] 7.1 `smart_stop_active = true` + PV Path A trigger + tank below ceiling → `Opportunity` intent returned (smart-stop does not block)
- [ ] 7.2 `smart_stop_active = true` + no PV trigger → `Idle` (smart-stop correctly blocks non-opportunity heating)
- [ ] 7.3 `smart_stop_active = true` + `override_smart_stop = false` in config → `Idle` even when PV trigger fires
- [ ] 7.4 Priority ordering test: `Boost` outranks `Opportunity`; `Opportunity` outranks `Preheat`; `Preheat` outranks `Idle` — write a priority-rank helper and test all adjacent pairs

## 8. Solar thermal interaction tests

- [ ] 8.1 `SolarPumped` system, collector 75°C, tank 55°C (< 65°C ceiling), pump active → solar thermal useful but below ceiling → `Opportunity` proceeds in parallel
- [ ] 8.2 `SolarPumped` system, collector 72°C, tank 66°C (> 65°C ceiling), pump active → `solar_thermal_suppress` fires → `Idle`
- [ ] 8.3 `SolarPumped` system, `collector_temp_c = None` → `solar_thermal_doing_useful_work` returns `false` → no suppression from solar thermal path
- [ ] 8.4 `ElectricOnly` system → `solar_thermal_doing_useful_work` always `false` (pump will be `None`)

## 9. Integration smoke test (`tests/opportunity_engine_integration.rs`)

- [ ] 9.1 Create `crates/smart-geyser-core/tests/opportunity_engine_integration.rs`
- [ ] 9.2 Scenario: 6-hour synthetic PV state stream — morning cloud (no trigger), midday peak (Path A fires, session starts), afternoon cloud dip (SOC drops 4%, min_run elapsed, session ends), late-afternoon recovery (new session starts on Path B)
- [ ] 9.3 Assert: opportunity sessions started and stopped at expected timestamps; `opportunity_active` flag transitions in `SharedEngineState` match
- [ ] 9.4 Scenario: smart-stop active all day + sunny afternoon → opportunity engine fires at least one session; decision engine emits no `Preheat` while smart-stop active
- [ ] 9.5 Assert: over the 6-hour stream, total simulated time with `opportunity_active = true` correlates with PV surplus windows

## ~~Tasks 1, 3–9~~ — Deferred to v2

The full `OpportunityEngine` (solar window, trigger paths A/B/C, suppress conditions, anti-cycling, integration tests) is deferred. The `None` PV case is handled at the service layer: when no `PVSystemProvider` is configured the `OpportunityEngine` is never constructed and `opportunity_active` stays `false`. `DecisionIntent::Opportunity` and `OpportunityReason` exist in the type system for when the engine is built.

## 10. Phase 3 wrap-up

- [x] 10.1 Inside the dev container: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` all green (per CLAUDE.md Docker-only rule)
- [x] 10.2 Tag commit `phase-3-complete`
- [x] 10.3 Update [CLAUDE.md](../CLAUDE.md) with any architectural decisions or learnings from this phase
- [x] 10.4 Open Phase 4 task file when starting that phase
