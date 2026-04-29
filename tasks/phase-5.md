# Phase 5 — Service & Add-on

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 5, §7 (Service API), §4 (SharedEngineState, priority order), §5 (repo structure for smart-geyser-service)

**Goal:** Wire the engines and providers into a running service. `smart-geyser-service` exposes an axum REST API, runs the decision and opportunity engines concurrently on a configurable tick interval, loads config from TOML/environment, and packages as a Docker image ready for HA add-on installation.

**Exit criteria:** `docker compose run --rm dev cargo test --workspace` passes. The service binary starts, both engines tick, all endpoints from spec §7 return valid JSON, and the Docker image builds and passes a smoke test. The HA add-on manifest is valid.

---

## 1. `smart-geyser-service` crate scaffolding

- [ ] 1.1 Create `crates/smart-geyser-service/` with `Cargo.toml`; add it to the workspace `members` list
- [ ] 1.2 Add dependencies: `smart-geyser-core` (path), `smart-geyser-providers` (path), `axum`, `tokio` (full features), `tower`, `tower-http` (tracing, CORS), `serde`, `serde_json`, `toml`, `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`, `chrono`
- [ ] 1.3 Create `src/main.rs` — parse config, build providers, start scheduler, start axum server
- [ ] 1.4 Create module skeleton: `src/config.rs`, `src/scheduler.rs`, `src/api/mod.rs`, `src/api/state.rs`, `src/api/status.rs`, `src/api/opportunity.rs`, `src/api/boost.rs`
- [ ] 1.5 Add dev-dependencies: `axum-test` (or `tower::ServiceExt` for request testing), `tokio-test`
- [ ] 1.6 Confirm `docker compose run --rm dev cargo build --workspace` succeeds with stubs

## 2. Configuration loading (`config.rs`)

- [ ] 2.1 Define `ServiceConfig` struct covering: `listen_addr: SocketAddr`, `tick_interval_secs: u32` (default: 60), `geyser_provider: GeyserProviderConfig` (enum over all supported providers), `pv_provider: Option<PvProviderConfig>`, `engine: EngineConfig` (from core), `data_dir: PathBuf` (for pattern store persistence)
- [ ] 2.2 Define `GeyserProviderConfig` and `PvProviderConfig` as `#[serde(tag = "type")]` enums covering all providers from Phase 4
- [ ] 2.3 Implement `ServiceConfig::load(path: &Path) -> anyhow::Result<ServiceConfig>`: read TOML file; allow any field to be overridden by environment variables (prefix `SMART_GEYSER_`)
- [ ] 2.4 Provide a `config.example.toml` in the repo root showing all fields with their defaults and comments
- [ ] 2.5 Unit tests: valid TOML round-trips correctly; missing required fields return descriptive `Err`; env override takes precedence over file value

## 3. Scheduler (`scheduler.rs`)

- [ ] 3.1 Define `Scheduler` struct holding `Arc<RwLock<SharedEngineState>>`, `DecisionEngine`, `OpportunityEngine`, boxed `GeyserProvider`, optional boxed `PVSystemProvider`
- [ ] 3.2 Implement `Scheduler::run(self, tick_interval: Duration)` — async loop that runs both engines on each tick:
  1. `get_state()` from the geyser provider
  2. `get_pv_state()` from the PV provider (if configured)
  3. `decision_engine.tick(geyser_state, now)` → `DecisionIntent`
  4. `opportunity_engine.tick(pv_state, geyser_state, now)` → `DecisionIntent` (only if PV configured)
  5. Resolve final intent per spec §4 priority order (BOOST > FAULT > OPPORTUNITY > PREHEAT > IDLE)
  6. Call `set_element(true/false)` on the geyser provider based on resolved intent
  7. Persist `PatternStore` to `data_dir` after feeding any new `UseEvent`
- [ ] 3.3 Log each tick at `DEBUG` level with geyser state, PV state (if any), resolved intent, and element command issued
- [ ] 3.4 On provider `Err`, log `WARN` and skip the tick without crashing the loop
- [ ] 3.5 Spawn the scheduler as a `tokio::task` in `main.rs`; ensure graceful shutdown on SIGTERM/SIGINT
- [ ] 3.6 Unit tests with mock providers (fixed responses) and a fixed clock:
  - Tick with PV opportunity conditions met → element turned on, `opportunity_active = true` in shared state
  - Tick with `Idle` intent → element turned off
  - Provider error on tick → warning logged, loop continues to next tick, element state unchanged

## 4. REST API — status endpoint (`api/status.rs`)

- [ ] 4.1 `GET /api/status` → full status JSON matching spec §7.2 exactly (field names are the contract — do not rename)
- [ ] 4.2 Response includes: `system_type`, `provider`, `tank_temp_c`, `collector_temp_c`, `pump_active`, `heating_active`, `smart_stop_active`, `preheat_active`, `pv` object (all PV fields from spec §7.2), `next_predicted_use`, `preheat_starts_at`, `events_today`, `energy_today`, `energy_30d`
- [ ] 4.3 `pv` object is omitted (or `null`) when no PV provider is configured
- [ ] 4.4 `opportunity_reason` uses the string values from `OpportunityReason` enum: `"battery_full_exporting"`, `"battery_full_pv_coverage"`, `"battery_full_soc_only"`
- [ ] 4.5 Unit test: mock scheduler state → `GET /api/status` → assert response body matches expected JSON snapshot

## 5. REST API — PV state endpoint (`api/state.rs`)

- [ ] 5.1 `GET /api/pv-state` → latest `PVSystemState` as JSON; returns 404 with `{"error": "no_pv_provider"}` when no PV provider is configured
- [ ] 5.2 Response includes `timestamp`, all `PVSystemState` fields (optional fields present only when `Some`)
- [ ] 5.3 Unit test: PV provider configured → 200 with full state; no PV provider → 404 with error body

## 6. REST API — opportunity log endpoint (`api/opportunity.rs`)

- [ ] 6.1 Define `OpportunitySession` struct: `started_at`, `ended_at` (optional if still active), `reason: OpportunityReason`, `peak_temp_c`, `estimated_kwh`
- [ ] 6.2 Maintain a rolling log of the last 48 sessions in memory (bounded `VecDeque`)
- [ ] 6.3 `GET /api/opportunity-log` → JSON array of `OpportunitySession` ordered newest-first
- [ ] 6.4 Scheduler writes to the log when a session starts and when it ends (updates `ended_at`)
- [ ] 6.5 Unit test: simulate 3 complete sessions + 1 active → log returns 4 entries; `ended_at` is `null` for the active session

## 7. REST API — boost and setpoint control (`api/boost.rs`)

- [ ] 7.1 `POST /api/boost` → accepts `{"duration_minutes": 60}`; sets `shared_state.boost_until = now + duration`; returns `{"ok": true, "boost_until": "<iso8601>"}`
- [ ] 7.2 `POST /api/setpoint` → accepts `{"temp_c": 62.0}`; updates `engine_config.setpoint_c` in the running scheduler; returns `{"ok": true}`
- [ ] 7.3 `DELETE /api/boost` → clears `boost_until`; returns `{"ok": true}`
- [ ] 7.4 Input validation: `duration_minutes` must be 1–480; `temp_c` must be 40.0–75.0; return 422 with descriptive error on invalid input
- [ ] 7.5 Unit tests: valid boost sets shared state correctly; out-of-range duration returns 422; `DELETE /api/boost` clears the field

## 8. Docker production image

- [ ] 8.1 Create `docker/Dockerfile.prod` — multi-stage build: stage 1 builds the release binary using the dev image; stage 2 is `debian:bookworm-slim` with only the binary and runtime dependencies
- [ ] 8.2 Expose port 8080; set `WORKDIR /data`; set `CMD ["/usr/local/bin/smart-geyser-service", "--config", "/data/options.json"]`
- [ ] 8.3 Add `docker/Dockerfile.prod` as a `prod` service in `docker-compose.yml` for local build testing
- [ ] 8.4 Smoke test: `docker compose build prod && docker compose run --rm prod smart-geyser-service --help` exits 0
- [ ] 8.5 Verify the production image has no `cargo`, `rustc`, or build toolchain binaries (keep it small)

## 9. HA add-on manifest

- [ ] 9.1 Create `addon/config.yaml` — standard HA add-on manifest: `name`, `version`, `description`, `url`, `arch: [aarch64, amd64, armhf]`, `startup: application`, `boot: auto`, `ports: {8080/tcp: 8080}`, `options` schema (mirrors `ServiceConfig`)
- [ ] 9.2 Create `addon/DOCS.md` — user-facing add-on documentation: installation, config options reference, first-run walkthrough
- [ ] 9.3 Create `addon/icon.png` and `addon/logo.png` (placeholder SVG-derived PNGs are acceptable for Phase 5)
- [ ] 9.4 Validate manifest with the HA add-on check tool (`docker run --rm -v $(pwd)/addon:/data ghcr.io/home-assistant/builder --check`)
- [ ] 9.5 `addon/config.yaml` `options` schema must expose at minimum: geyser provider type + connection details, PV provider type + connection details (optional), `setpoint_c`, `opportunity_max_temp_c`, `soc_full_threshold_pct`

## 10. Integration smoke test

- [ ] 10.1 Create `tests/service_smoke.rs` or a shell script: start the service binary with a minimal config (mock providers via config), hit all endpoints, assert HTTP 200 and valid JSON bodies
- [ ] 10.2 Assert `GET /api/status` returns the `system_type` matching the configured provider
- [ ] 10.3 Assert `POST /api/boost` → `GET /api/status` shows `boost_until` in the future
- [ ] 10.4 Assert `DELETE /api/boost` → `GET /api/status` shows `boost_until` absent

## 11. Phase 5 wrap-up

- [ ] 11.1 Inside the dev container: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` all green
- [ ] 11.2 Production Docker image builds and smoke test passes
- [ ] 11.3 HA add-on manifest validates without errors
- [ ] 11.4 Tag commit `phase-5-complete`
- [ ] 11.5 Update [CLAUDE.md](../CLAUDE.md) with any service-level architectural decisions
- [ ] 11.6 Open Phase 6 task file when starting that phase
